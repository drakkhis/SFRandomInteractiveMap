use crate::models::WorldSnapshot;
use crate::{parse_save_bytes, parse_save_file};
use anyhow::{Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub samples_dir: PathBuf,
    pub active_save_filename: Arc<RwLock<Option<String>>>,
    pub cache: Arc<RwLock<Option<(SystemTime, WorldSnapshot)>>>,
    pub generated_map_png: Arc<RwLock<Option<Vec<u8>>>>,
    pub generated_heightmap: Arc<RwLock<Option<Vec<u8>>>>,
    pub fog_of_war_png: Arc<RwLock<Option<Vec<u8>>>>,
}

impl AppState {
    pub fn new(samples_dir: impl AsRef<Path>) -> Self {
        Self {
            samples_dir: samples_dir.as_ref().to_path_buf(),
            active_save_filename: Arc::new(RwLock::new(None)),
            cache: Arc::new(RwLock::new(None)),
            generated_map_png: Arc::new(RwLock::new(None)),
            generated_heightmap: Arc::new(RwLock::new(None)),
            fog_of_war_png: Arc::new(RwLock::new(None)),
        }
    }
}

/// Helper to find the newest .sav file in a directory by modification timestamp.
pub fn find_latest_save(dir: &Path) -> Result<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory {:?}", dir))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "sav"))
        .collect();

    if entries.is_empty() {
        anyhow::bail!("No .sav files found in directory {:?}", dir);
    }

    entries.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });

    Ok(entries.pop().unwrap())
}

/// Helper to determine the target save path (active selection or latest save file).
pub async fn get_target_save_path(state: &AppState) -> Result<PathBuf> {
    let active_name = state.active_save_filename.read().await.clone();
    if let Some(name) = active_name {
        let p = state.samples_dir.join(&name);
        if p.exists() {
            return Ok(p);
        }
    }
    find_latest_save(&state.samples_dir)
}

#[derive(Serialize, Deserialize)]
pub struct ApiStatus {
    pub status: String,
    pub latest_save: Option<String>,
    pub session_name: Option<String>,
    pub world_seed: Option<i32>,
    pub world_size_meters: Option<f64>,
    pub total_nodes: Option<usize>,
    pub total_somersloops: Option<usize>,
    pub total_mercer_spheres: Option<usize>,
}

/// Information about a save file in the samples/ directory.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub modified_timestamp: u64,
    pub is_active: bool,
}

/// Handler for GET /api/saves.
pub async fn list_saves_handler(State(state): State<AppState>) -> impl IntoResponse {
    let active_opt = state.active_save_filename.read().await.clone();
    let current_active = active_opt;

    let mut entries = Vec::new();
    if let Ok(dir_entries) = fs::read_dir(&state.samples_dir) {
        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "sav") {
                if let Ok(meta) = fs::metadata(&path) {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let is_active = current_active.as_deref() == Some(&filename);
                    let mtime = meta
                        .modified()
                        .unwrap_or(SystemTime::UNIX_EPOCH)
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    entries.push(SaveEntry {
                        filename,
                        size_bytes: meta.len(),
                        modified_timestamp: mtime,
                        is_active,
                    });
                }
            }
        }
    }

    entries.sort_by(|a, b| b.modified_timestamp.cmp(&a.modified_timestamp));
    Json(entries)
}

#[derive(Deserialize)]
pub struct UploadQuery {
    pub filename: Option<String>,
}

/// Handler for POST /api/upload.
/// Uploads a .sav file directly from the browser, writes it to samples/, and parses it immediately.
pub async fn upload_save_handler(
    State(state): State<AppState>,
    Query(params): Query<UploadQuery>,
    body: axum::body::Bytes,
) -> Result<Json<WorldSnapshot>, (StatusCode, String)> {
    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Uploaded save file is empty".to_string(),
        ));
    }

    let raw_name = params.filename.unwrap_or_else(|| {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("upload_{}.sav", ts)
    });

    let safe_filename = Path::new(&raw_name)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "uploaded_save.sav".to_string());

    let _ = fs::create_dir_all(&state.samples_dir);
    let target_path = state.samples_dir.join(&safe_filename);

    // Save uploaded bytes to disk
    fs::write(&target_path, &body).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed writing save file to disk: {}", e),
        )
    })?;

    // Parse bytes directly in background thread
    let body_bytes = body.to_vec();
    let summary = tokio::task::spawn_blocking(move || parse_save_bytes(&body_bytes))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed parsing uploaded save: {}", e)))?;

    let snapshot = summary.to_world_snapshot();
    let mtime = fs::metadata(&target_path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| SystemTime::now());

    // Update active save, generated map PNG, and cache
    {
        let mut active = state.active_save_filename.write().await;
        *active = Some(safe_filename);
    }
    {
        let mut map_lock = state.generated_map_png.write().await;
        *map_lock = summary.generated_map_png;
    }
    {
        let mut hm_lock = state.generated_heightmap.write().await;
        *hm_lock = summary.generated_heightmap;
    }
    {
        let mut fog_lock = state.fog_of_war_png.write().await;
        *fog_lock = summary.fog_of_war_png;
    }
    {
        let mut cached = state.cache.write().await;
        *cached = Some((mtime, snapshot.clone()));
    }

    Ok(Json(snapshot))
}

#[derive(Deserialize)]
pub struct LoadQuery {
    pub filename: String,
}

/// Handler for POST /api/load.
/// Switches to and loads an existing .sav file in samples/.
pub async fn load_save_handler(
    State(state): State<AppState>,
    Query(params): Query<LoadQuery>,
) -> Result<Json<WorldSnapshot>, (StatusCode, String)> {
    let safe_filename = Path::new(&params.filename)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid filename".to_string()))?;

    let target_path = state.samples_dir.join(&safe_filename);
    if !target_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Save file {:?} not found", safe_filename),
        ));
    }

    let path_clone = target_path.clone();
    let summary = tokio::task::spawn_blocking(move || parse_save_file(&path_clone))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task join error: {}", e)))?
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed parsing save file: {}", e)))?;

    let snapshot = summary.to_world_snapshot();
    let mtime = fs::metadata(&target_path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::now());

    {
        let mut active = state.active_save_filename.write().await;
        *active = Some(safe_filename);
    }
    {
        let mut map_lock = state.generated_map_png.write().await;
        *map_lock = summary.generated_map_png;
    }
    {
        let mut hm_lock = state.generated_heightmap.write().await;
        *hm_lock = summary.generated_heightmap;
    }
    {
        let mut fog_lock = state.fog_of_war_png.write().await;
        *fog_lock = summary.fog_of_war_png;
    }
    {
        let mut cached = state.cache.write().await;
        *cached = Some((mtime, snapshot.clone()));
    }

    Ok(Json(snapshot))
}

/// Handler for GET /api/status.
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let target_save = get_target_save_path(&state).await.ok();
    let latest_filename = target_save
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().to_string());

    let cached = state.cache.read().await;
    let (session_name, world_seed, world_size_meters, total_nodes, total_somersloops, total_mercer_spheres) = if let Some((_, ref snap)) = *cached {
        (
            Some(snap.session_name.clone()),
            snap.world_seed,
            snap.world_size_meters,
            Some(snap.total_nodes),
            Some(snap.total_somersloops),
            Some(snap.total_mercer_spheres),
        )
    } else {
        (None, None, None, None, None, None)
    };

    Json(ApiStatus {
        status: "ok".to_string(),
        latest_save: latest_filename,
        session_name,
        world_seed,
        world_size_meters,
        total_nodes,
        total_somersloops,
        total_mercer_spheres,
    })
}

/// Handler for GET /api/world.
pub async fn world_handler(
    State(state): State<AppState>,
) -> Result<Json<WorldSnapshot>, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let mtime = fs::metadata(&target_save)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    // 1. Check cache
    {
        let cached = state.cache.read().await;
        if let Some((cached_mtime, ref snapshot)) = *cached {
            if cached_mtime == mtime {
                return Ok(Json(snapshot.clone()));
            }
        }
    }

    // 2. Parse on demand
    let path_clone = target_save.clone();
    let summary = tokio::task::spawn_blocking(move || parse_save_file(&path_clone))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task join error: {}", e),
            )
        })?
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed parsing save file {:?}: {}", target_save, e),
            )
        })?;

    let snapshot = summary.to_world_snapshot();

    // 3. Update generated map and cache
    {
        let mut map_lock = state.generated_map_png.write().await;
        *map_lock = summary.generated_map_png;
    }
    {
        let mut hm_lock = state.generated_heightmap.write().await;
        *hm_lock = summary.generated_heightmap;
    }
    {
        let mut fog_lock = state.fog_of_war_png.write().await;
        *fog_lock = summary.fog_of_war_png;
    }
    {
        let mut cached = state.cache.write().await;
        *cached = Some((mtime, snapshot.clone()));
    }

    Ok(Json(snapshot))
}

const INDEX_HTML: &str = include_str!("static/index.html");
const LANDMASS_PNG: &[u8] = include_bytes!("static/landmass.png");
const VANILLA_MAP_PNG: &[u8] = include_bytes!("static/vanilla_map.png");
const VANILLA_HEIGHTMAP_BIN: &[u8] = include_bytes!("static/vanilla_heightmap.bin");

/// Handler for GET / returning the interactive Leaflet map frontend.
pub async fn index_handler() -> impl IntoResponse {
    axum::response::Html(INDEX_HTML)
}

/// Handler for GET /static/landmass.png and GET /api/map.png returning the dynamically
/// generated procedural landmass PNG for RWG saves, or the standard map PNG for Persistent_Level saves.
pub async fn landmass_handler(State(state): State<AppState>) -> impl IntoResponse {
    let map_opt = state.generated_map_png.read().await.clone();
    let bytes = if let Some(b) = map_opt {
        b
    } else {
        let is_vanilla = {
            let cached = state.cache.read().await;
            cached
                .as_ref()
                .map(|(_, snap)| snap.map_name == "Persistent_Level" || snap.world_seed.is_none())
                .unwrap_or(false)
        };
        if is_vanilla {
            VANILLA_MAP_PNG.to_vec()
        } else {
            LANDMASS_PNG.to_vec()
        }
    };
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (axum::http::header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        bytes,
    )
}

/// Handler for GET /api/heightmap.bin returning the compact 512x512 binary `PWHM` 3D heightmap
/// for RWG saves (extracted from `PWW1` Section 1 & 2) or Standard Map saves (`vanilla_heightmap.bin`).
pub async fn heightmap_handler(State(state): State<AppState>) -> impl IntoResponse {
    let hm_opt = state.generated_heightmap.read().await.clone();
    let bytes = hm_opt.unwrap_or_else(|| VANILLA_HEIGHTMAP_BIN.to_vec());
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
            (axum::http::header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        bytes,
    )
}

/// Request payload to relocate the pioneer player character.
#[derive(Deserialize)]
pub struct TeleportRequest {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Handler for POST /api/save/teleport.
/// Recompresses the active save file with updated player coordinates and returns the modified .sav binary.
pub async fn teleport_handler(
    State(state): State<AppState>,
    Json(payload): Json<TeleportRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_teleported.sav", base_stem);

    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let (repacked_bytes, _players) = tokio::task::spawn_blocking(move || {
        crate::modify_player_coordinates(&raw_bytes, payload.x, payload.y, payload.z)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to modify player position: {}", e)))?;

    // Also write a copy into samples/ directory so it appears in save lists and can be loaded directly
    let modified_save_path = state.samples_dir.join(&new_filename);
    let _ = fs::write(&modified_save_path, &repacked_bytes);

    // Update cached snapshot player position so frontend immediately reflects new coordinates
    {
        let mut cached_lock = state.cache.write().await;
        if let Some((_, ref mut snapshot)) = *cached_lock {
            if let Some(ref mut p) = snapshot.player {
                p.transform.translation = [payload.x, payload.y, payload.z];
            }
        }
    }

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-teleport-filename"),
            new_filename,
        ),
    ];

    Ok((headers, repacked_bytes))
}

/// Request payload to move a resource node.
#[derive(Deserialize)]
pub struct MoveNodeRequest {
    pub instance_name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation: Option<[f32; 4]>,
}

#[derive(Serialize)]
pub struct MoveNodeResponse {
    pub status: String,
    pub moved_count: usize,
    pub moved_nodes: Vec<crate::models::ResourceNodeSummary>,
}

/// Handler for POST /api/node/move.
/// Modifies coordinates (and optional slope rotation) of a resource node (and any satellites if a ResourceWellCore).
/// Automatically saves the modified state into the active save file on disk and updates cache.
pub async fn move_node_handler(
    State(state): State<AppState>,
    Json(payload): Json<MoveNodeRequest>,
) -> Result<Json<MoveNodeResponse>, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let (repacked_bytes, moved_nodes) = tokio::task::spawn_blocking(move || {
        crate::modify_node_coordinates(
            &raw_bytes,
            &payload.instance_name,
            payload.x,
            payload.y,
            payload.z,
            payload.rotation,
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to move node: {}", e)))?;

    // Save modified bytes directly to the active save file on disk
    fs::write(&target_save, &repacked_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed writing modified save to disk: {}", e),
        )
    })?;

    let new_mtime = fs::metadata(&target_save)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| SystemTime::now());

    // Update in-memory snapshot cache
    {
        let mut cached_lock = state.cache.write().await;
        if let Some((ref mut mtime, ref mut snapshot)) = *cached_lock {
            *mtime = new_mtime;
            for moved in &moved_nodes {
                if let Some(node) = snapshot.resource_nodes.iter_mut().find(|n| {
                    n.instance_name == moved.instance_name
                        || n.instance_name.rsplit('.').next().unwrap_or("") == moved.instance_name.rsplit('.').next().unwrap_or("")
                        || n.instance_name.ends_with(&moved.instance_name)
                        || moved.instance_name.ends_with(&n.instance_name)
                }) {
                    node.transform.translation = moved.transform.translation;
                    node.transform.rotation = moved.transform.rotation;
                }
            }
        }
    }

    Ok(Json(MoveNodeResponse {
        status: "ok".to_string(),
        moved_count: moved_nodes.len(),
        moved_nodes,
    }))
}

/// Handler for POST /api/player/move.
/// Modifies coordinates of the player character and saves back to disk and cache without initiating download.
pub async fn move_player_handler(
    State(state): State<AppState>,
    Json(payload): Json<TeleportRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let (repacked_bytes, _players) = tokio::task::spawn_blocking(move || {
        crate::modify_player_coordinates(&raw_bytes, payload.x, payload.y, payload.z)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to modify player position: {}", e)))?;

    // Save modified bytes directly to active save file on disk
    fs::write(&target_save, &repacked_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed writing modified save to disk: {}", e),
        )
    })?;

    let new_mtime = fs::metadata(&target_save)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| SystemTime::now());

    // Update cached snapshot player position
    {
        let mut cached_lock = state.cache.write().await;
        if let Some((ref mut mtime, ref mut snapshot)) = *cached_lock {
            *mtime = new_mtime;
            if let Some(ref mut p) = snapshot.player {
                p.transform.translation = [payload.x, payload.y, payload.z];
            }
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "x": payload.x,
        "y": payload.y,
        "z": payload.z
    })))
}

/// Handler for GET /api/save/download.
/// Downloads the active .sav file directly from disk.
pub async fn download_save_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let filename = target_save
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "save.sav".to_string());

    let bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            filename,
        ),
    ];

    Ok((headers, bytes))
}

#[derive(Deserialize)]
pub struct MoveStructuresRequest {
    pub instance_names: Vec<String>,
    pub dx: f32,
    pub dy: f32,
    pub dz: f32,
}

/// Handler for POST /api/save/move-structures.
pub async fn move_structures_handler(
    State(state): State<AppState>,
    Json(payload): Json<MoveStructuresRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_moved_structures.sav", base_stem);
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;
    let names = payload.instance_names.clone();
    let (repacked_bytes, moved_count) = tokio::task::spawn_blocking(move || {
        crate::modify_structures_offset(&raw_bytes, &names, payload.dx, payload.dy, payload.dz)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to move structures: {}", e)))?;

    {
        use std::collections::HashSet;
        let targets: HashSet<&str> = payload.instance_names.iter().map(|s| s.as_str()).collect();
        let mut cached_lock = state.cache.write().await;
        if let Some((_, ref mut snapshot)) = *cached_lock {
            for st in &mut snapshot.player_structures {
                if targets.contains(st.instance_name.as_str()) {
                    st.transform.translation[0] += payload.dx;
                    st.transform.translation[1] += payload.dy;
                    st.transform.translation[2] += payload.dz;
                }
            }
        }
    }

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-moved-count"),
            moved_count.to_string(),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            new_filename,
        ),
    ];
    Ok((headers, repacked_bytes))
}

/// Handler for GET /api/fog.png — returns the active save's Fog of War PNG overlay.
pub async fn fog_handler(State(state): State<AppState>) -> impl IntoResponse {
    // 1x1 transparent PNG fallback
    const TRANSPARENT_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let png = state
        .fog_of_war_png
        .read()
        .await
        .clone()
        .unwrap_or_else(|| TRANSPARENT_PNG.to_vec());
    (
        [(axum::http::header::CONTENT_TYPE, "image/png".to_string())],
        png,
    )
}

/// Handler for GET /api/items/known — returns the built-in base-game + mod item catalog.
pub async fn known_items_handler() -> impl IntoResponse {
    Json(crate::item_registry::all_known_items())
}

#[derive(Deserialize)]
pub struct ScanModsRequest {
    pub mods_dir: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ScanModsResponse {
    pub scanned_dir: String,
    pub scanned_count: usize,
    pub items: Vec<crate::item_registry::KnownItemEntry>,
}

/// Handler for POST /api/mods/scan — scans a Satisfactory Mods folder for item blueprints.
pub async fn scan_mods_handler(
    Json(payload): Json<ScanModsRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dir_str = payload.mods_dir.unwrap_or_else(|| {
        r"C:\Program Files (x86)\Steam\steamapps\common\Satisfactory\FactoryGame\Mods".to_string()
    });
    let dir_path = PathBuf::from(&dir_str);
    let items = tokio::task::spawn_blocking(move || crate::mod_scanner::scan_mods_folder(&dir_path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Mod scan failed: {}", e)))?;
    let scanned_count = items.len();
    Ok(Json(ScanModsResponse {
        scanned_dir: dir_str,
        scanned_count,
        items,
    }))
}

#[derive(Deserialize)]
pub struct EditInventoryRequest {
    pub component_instance_name: String,
    pub edits: Vec<crate::InventorySlotEdit>,
}

/// Handler for POST /api/save/edit-inventory.
pub async fn edit_inventory_handler(
    State(state): State<AppState>,
    Json(payload): Json<EditInventoryRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_edited_inv.sav", base_stem);
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;
    let comp_name = payload.component_instance_name.clone();
    let edits = payload.edits.clone();
    let repacked_bytes = tokio::task::spawn_blocking(move || {
        crate::modify_inventory_slots(&raw_bytes, &comp_name, &edits)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to edit inventory: {}", e)))?;

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            new_filename,
        ),
    ];
    Ok((headers, repacked_bytes))
}

/// Handler for POST /api/save/clear-fog.
pub async fn clear_fog_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_nofog.sav", base_stem);
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;
    let repacked_bytes = tokio::task::spawn_blocking(move || crate::clear_map_fog(&raw_bytes))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to clear fog: {}", e)))?;

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            new_filename,
        ),
    ];
    Ok((headers, repacked_bytes))
}

#[derive(Deserialize)]
pub struct GameSettingsRequest {
    pub is_creative_mode_enabled: Option<bool>,
    #[serde(default)]
    pub updates: Vec<crate::GameRuleUpdate>,
}

/// Handler for POST /api/save/game-settings.
pub async fn game_settings_handler(
    State(state): State<AppState>,
    Json(payload): Json<GameSettingsRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_settings.sav", base_stem);
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;
    let creative = payload.is_creative_mode_enabled;
    let updates = payload.updates.clone();
    let repacked_bytes = tokio::task::spawn_blocking(move || {
        crate::modify_game_settings(&raw_bytes, creative, &updates)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update game settings: {}", e)))?;

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            new_filename,
        ),
    ];
    Ok((headers, repacked_bytes))
}

#[derive(Deserialize)]
pub struct CompileSaveRequest {
    #[serde(default)]
    pub clear_fog: bool,
    #[serde(default)]
    pub game_settings: Option<GameSettingsRequest>,
    #[serde(default)]
    pub inventory_edits: Vec<EditInventoryRequest>,
    #[serde(default)]
    pub move_structures: Vec<MoveStructuresRequest>,
    #[serde(default)]
    pub duplicate_structures: Vec<MoveStructuresRequest>,
    #[serde(default)]
    pub delete_structures: Vec<Vec<String>>,
}

async fn compile_save_bytes(
    state: &AppState,
    payload: CompileSaveRequest,
) -> Result<(String, Vec<u8>), (StatusCode, String)> {
    let target_save = get_target_save_path(state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let base_stem = target_save
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let new_filename = format!("{}_edited.sav", base_stem);
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let clear_fog = payload.clear_fog;
    let game_settings = payload.game_settings;
    let inventory_edits = payload.inventory_edits;
    let move_structures = payload.move_structures;
    let duplicate_structures = payload.duplicate_structures;
    let delete_structures = payload.delete_structures;

    let compiled_bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut current = raw_bytes;
        if clear_fog {
            current = crate::clear_map_fog(&current)?;
        }
        if let Some(ref gs) = game_settings {
            current = crate::modify_game_settings(
                &current,
                gs.is_creative_mode_enabled,
                &gs.updates,
            )?;
        }
        for inv_req in &inventory_edits {
            if !inv_req.edits.is_empty() {
                current = crate::modify_inventory_slots(
                    &current,
                    &inv_req.component_instance_name,
                    &inv_req.edits,
                )?;
            }
        }
        for mv in &move_structures {
            if !mv.instance_names.is_empty() {
                let (next, _) = crate::modify_structures_offset(
                    &current,
                    &mv.instance_names,
                    mv.dx,
                    mv.dy,
                    mv.dz,
                )?;
                current = next;
            }
        }
        for dup in &duplicate_structures {
            if !dup.instance_names.is_empty() {
                let (next, _) = crate::duplicate_structures_in_save(
                    &current,
                    &dup.instance_names,
                    dup.dx,
                    dup.dy,
                    dup.dz,
                )?;
                current = next;
            }
        }
        for del_names in &delete_structures {
            if !del_names.is_empty() {
                let (next, _) = crate::delete_structures_in_save(&current, del_names)?;
                current = next;
            }
        }
        Ok(current)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to compile save: {}", e)))?;

    Ok((new_filename, compiled_bytes))
}

/// Handler for POST /api/save/compile — applies all staged edits (fog, game settings,
/// inventory slots, structure moves) in one pass and returns the compiled `.sav` download.
pub async fn compile_save_handler(
    State(state): State<AppState>,
    Json(payload): Json<CompileSaveRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (new_filename, compiled_bytes) = compile_save_bytes(&state, payload).await?;
    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", new_filename),
        ),
        (
            axum::http::header::HeaderName::from_static("x-save-filename"),
            new_filename,
        ),
    ];
    Ok((headers, compiled_bytes))
}

fn find_satisfactory_save_dir() -> Result<PathBuf, String> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA environment variable not found".to_string())?;
    let save_games_root = PathBuf::from(local_app_data)
        .join("FactoryGame")
        .join("Saved")
        .join("SaveGames");
    if !save_games_root.exists() {
        return Err(format!(
            "Satisfactory SaveGames folder not found at {:?}",
            save_games_root
        ));
    }

    let mut best_dir: Option<(SystemTime, PathBuf)> = None;
    let mut first_profile_dir: Option<PathBuf> = None;

    if let Ok(entries) = fs::read_dir(&save_games_root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let dir_name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if dir_name.eq_ignore_ascii_case("server") {
                continue;
            }
            if first_profile_dir.is_none() {
                first_profile_dir = Some(p.clone());
            }
            if let Ok(sub_entries) = fs::read_dir(&p) {
                for sub in sub_entries.flatten() {
                    let sp = sub.path();
                    if sp.extension().and_then(|e| e.to_str()) == Some("sav") {
                        if let Ok(meta) = sp.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                if best_dir.as_ref().map_or(true, |(t, _)| mtime > *t) {
                                    best_dir = Some((mtime, p.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    best_dir
        .map(|(_, d)| d)
        .or(first_profile_dir)
        .ok_or_else(|| format!("No profile save folder found inside {:?}", save_games_root))
}

#[derive(serde::Serialize)]
pub struct SaveToGameResponse {
    pub saved_path: String,
    pub filename: String,
}

/// Handler for POST /api/save/compile-to-game — compiles staged edits and writes directly
/// into the user's Satisfactory SaveGames profile folder, bypassing browser Save As restrictions.
pub async fn compile_to_game_handler(
    State(state): State<AppState>,
    Json(payload): Json<CompileSaveRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (new_filename, compiled_bytes) = compile_save_bytes(&state, payload).await?;
    let target_dir = find_satisfactory_save_dir().map_err(|e| (StatusCode::NOT_FOUND, e))?;
    let dest_path = target_dir.join(&new_filename);
    fs::write(&dest_path, &compiled_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write {:?}: {}", dest_path, e),
        )
    })?;
    Ok(Json(SaveToGameResponse {
        saved_path: dest_path.to_string_lossy().to_string(),
        filename: new_filename,
    }))
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

#[derive(Deserialize)]
pub struct ExportBlueprintRequest {
    pub instance_names: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub save_to_game: bool,
}

#[derive(serde::Serialize)]
pub struct ExportBlueprintResponse {
    pub filename: String,
    pub exported_count: usize,
    pub saved_dir: Option<String>,
    pub sbp_b64: String,
    pub sbpcfg_b64: String,
}

/// Handler for POST /api/blueprint/export — exports selected structures as `.sbp` + `.sbpcfg`.
pub async fn export_blueprint_handler(
    State(state): State<AppState>,
    Json(payload): Json<ExportBlueprintRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_save = get_target_save_path(&state).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;
    let raw_bytes = fs::read(&target_save).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read save file {:?}: {}", target_save, e),
        )
    })?;

    let safe_name: String = payload
        .name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let bp_name = if safe_name.is_empty() { "MapSelection".to_string() } else { safe_name };
    let names = payload.instance_names.clone();
    let desc = payload.description.clone();

    let (sbp_bytes, sbpcfg_bytes, exported_count) = tokio::task::spawn_blocking(move || {
        crate::export_selection_as_blueprint(&raw_bytes, &names, &desc)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Join error: {}", e)))?
    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Blueprint export failed: {}", e)))?;

    let mut saved_dir = None;
    if payload.save_to_game {
        let session_name = {
            let cached = state.cache.read().await;
            cached
                .as_ref()
                .map(|(_, s)| s.session_name.clone())
                .unwrap_or_else(|| "Default".to_string())
        };
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let bp_dir = PathBuf::from(local_app_data)
                .join("FactoryGame")
                .join("Saved")
                .join("SaveGames")
                .join("blueprints")
                .join(&session_name);
            if fs::create_dir_all(&bp_dir).is_ok() {
                let sbp_path = bp_dir.join(format!("{}.sbp", bp_name));
                let cfg_path = bp_dir.join(format!("{}.sbpcfg", bp_name));
                if fs::write(&sbp_path, &sbp_bytes).is_ok() && fs::write(&cfg_path, &sbpcfg_bytes).is_ok() {
                    saved_dir = Some(bp_dir.to_string_lossy().to_string());
                }
            }
        }
    }

    Ok(Json(ExportBlueprintResponse {
        filename: bp_name,
        exported_count,
        saved_dir,
        sbp_b64: encode_base64(&sbp_bytes),
        sbpcfg_b64: encode_base64(&sbpcfg_bytes),
    }))
}

/// Creates the axum router for the application.
pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/static/landmass.png", get(landmass_handler))
        .route("/api/map.png", get(landmass_handler))
        .route("/api/fog.png", get(fog_handler))
        .route("/api/heightmap.bin", get(heightmap_handler))
        .route("/api/status", get(status_handler))
        .route("/api/world", get(world_handler))
        .route("/api/saves", get(list_saves_handler))
        .route("/api/upload", post(upload_save_handler))
        .route("/api/load", post(load_save_handler))
        .route("/api/save/download", get(download_save_handler))
        .route("/api/save/teleport", post(teleport_handler))
        .route("/api/player/move", post(move_player_handler))
        .route("/api/node/move", post(move_node_handler))
        .route("/api/save/move-structures", post(move_structures_handler))
        .route("/api/save/clear-fog", post(clear_fog_handler))
        .route("/api/save/game-settings", post(game_settings_handler))
        .route("/api/items/known", get(known_items_handler))
        .route("/api/mods/scan", post(scan_mods_handler))
        .route("/api/save/edit-inventory", post(edit_inventory_handler))
        .route("/api/save/compile", post(compile_save_handler))
        .route("/api/save/compile-to-game", post(compile_to_game_handler))
        .route("/api/blueprint/export", post(export_blueprint_handler))
        .layer(DefaultBodyLimit::max(250 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}


