pub mod decompressor;
pub mod map_gen;
pub mod models;
pub mod parser;
pub mod save_writer;
pub mod server;
pub mod vanilla_nodes_data;

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub use decompressor::decompress_save_chunks;
pub use map_gen::{extract_and_render_map, GeneratedMapData};
pub use models::*;
pub use parser::{clean_purity, detect_mod_source, extract_clean_name, extract_rwg_summary, parse_save_game, parse_save_header};
pub use save_writer::{modify_node_coordinates, modify_player_coordinates, recompress_save};
pub use server::{create_app, find_latest_save, AppState};

/// Parses a Satisfactory save file directly from a raw byte buffer,
/// including dynamically extracting and rendering the embedded RWG procedural map.
pub fn parse_save_bytes(raw_bytes: &[u8]) -> Result<RwgActorsSummary> {
    let (header, chunk_offset) = parse_save_header(raw_bytes)
        .context("Failed parsing save file header")?;

    let decompressed = decompress_save_chunks(raw_bytes, chunk_offset)
        .context("Failed decompressing save file chunks")?;

    // Parse actor stream and generate procedural map in parallel
    let (parsed_res, map_res) = rayon::join(
        || parse_save_game(&decompressed.uncompressed_data, header),
        || extract_and_render_map(&decompressed.uncompressed_data, 1500),
    );

    let parsed_save = parsed_res.context("Failed parsing save game actor stream")?;
    let mut summary = extract_rwg_summary(&parsed_save);

    if let Ok(Some(map_data)) = map_res {
        // Use exact world size from PWW1 header if not already set
        if summary.world_size_meters.is_none() && map_data.world_extent_cm > 0.0 {
            summary.world_size_meters = Some((map_data.world_extent_cm * 2.0) / 100.0);
        }
        summary.generated_map_png = Some(map_data.png_bytes);
        summary.generated_heightmap = Some(map_data.heightmap_bytes);
    }

    Ok(summary)
}

/// Loads a Satisfactory save file from disk, parses headers, decompresses chunks,
/// parses all actors, and extracts the RWG actors summary.
pub fn parse_save_file<P: AsRef<Path>>(path: P) -> Result<RwgActorsSummary> {
    let raw_bytes = fs::read(path.as_ref())
        .with_context(|| format!("Failed to read save file at {:?}", path.as_ref()))?;
    parse_save_bytes(&raw_bytes)
}


