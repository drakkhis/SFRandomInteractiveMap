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
}

impl AppState {
    pub fn new(samples_dir: impl AsRef<Path>) -> Self {
        Self {
            samples_dir: samples_dir.as_ref().to_path_buf(),
            active_save_filename: Arc::new(RwLock::new(None)),
            cache: Arc::new(RwLock::new(None)),
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

    // Update active save and cache
    {
        let mut active = state.active_save_filename.write().await;
        *active = Some(safe_filename);
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

    // 3. Update cache
    {
        let mut cached = state.cache.write().await;
        *cached = Some((mtime, snapshot.clone()));
    }

    Ok(Json(snapshot))
}

const INDEX_HTML: &str = include_str!("static/index.html");
const LANDMASS_PNG: &[u8] = include_bytes!("static/landmass.png");

/// Handler for GET / returning the interactive Leaflet map frontend.
pub async fn index_handler() -> impl IntoResponse {
    axum::response::Html(INDEX_HTML)
}

/// Handler for GET /static/landmass.png returning the textured landmass image.
pub async fn landmass_handler() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "image/png")],
        LANDMASS_PNG,
    )
}

/// Creates the axum router for the application.
pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/static/landmass.png", get(landmass_handler))
        .route("/api/status", get(status_handler))
        .route("/api/world", get(world_handler))
        .route("/api/saves", get(list_saves_handler))
        .route("/api/upload", post(upload_save_handler))
        .route("/api/load", post(load_save_handler))
        .layer(DefaultBodyLimit::max(250 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}


