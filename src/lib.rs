pub mod decompressor;
pub mod models;
pub mod parser;
pub mod server;

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub use decompressor::decompress_save_chunks;
pub use models::*;
pub use parser::{extract_rwg_summary, parse_save_game, parse_save_header};
pub use server::{create_app, find_latest_save, AppState};

/// Loads a Satisfactory save file from disk, parses headers, decompresses chunks,
/// parses all actors, and extracts the RWG actors summary.
pub fn parse_save_file<P: AsRef<Path>>(path: P) -> Result<RwgActorsSummary> {
    let raw_bytes = fs::read(path.as_ref())
        .with_context(|| format!("Failed to read save file at {:?}", path.as_ref()))?;

    let (header, chunk_offset) = parse_save_header(&raw_bytes)
        .context("Failed parsing save file header")?;

    let decompressed = decompress_save_chunks(&raw_bytes, chunk_offset)
        .context("Failed decompressing save file chunks")?;

    let parsed_save = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed parsing save game actor stream")?;

    let summary = extract_rwg_summary(&parsed_save);
    Ok(summary)
}
