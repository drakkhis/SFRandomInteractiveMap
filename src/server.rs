use crate::models::WorldSnapshot;
use crate::parse_save_file;
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
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
    pub cache: Arc<RwLock<Option<(SystemTime, WorldSnapshot)>>>,
}

impl AppState {
    pub fn new(samples_dir: impl AsRef<Path>) -> Self {
        Self {
            samples_dir: samples_dir.as_ref().to_path_buf(),
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

/// Handler for GET /api/status.
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let latest_save_path = find_latest_save(&state.samples_dir).ok();
    let latest_filename = latest_save_path
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
    let latest_save = find_latest_save(&state.samples_dir).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("No save file found in {:?}: {}", state.samples_dir, e),
        )
    })?;

    let mtime = fs::metadata(&latest_save)
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
    let path_clone = latest_save.clone();
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
                format!("Failed parsing save file {:?}: {}", latest_save, e),
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
        .layer(CorsLayer::permissive())
        .with_state(state)
}

