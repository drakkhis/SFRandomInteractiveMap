# Project Specification: Satisfactory RandomWorldGenerator Map & Save Tool (Rust)

## 1. Objective
Build a high-performance Rust application (`sat-rwg-map`) that parses Satisfactory `.sav` files generated with the `RandomWorldGenerator` (RWG) mod, extracts dynamic world generation data, resource nodes, drop pods, and player structures, and serves a self-hosted interactive 2D map via a local web server (`axum`).

## 2. Reference Specifications (`./reference_specs/`)
Use `sav_parse.py` and `sav_to_resave.py` from `GreyHak/sat_sav_parse` (copied into `./reference_specs/`) as your binary format blueprint:
- Follow `sav_parse.py`'s chunk decompression rules (`PACKAGE_FILE_TAG = 0x9E2A83C1`, zlib decompression via `flate2`).
- Follow ActorHeader, ObjectHeader, and property tree structures.
- **Critical Mod Trait:** The `RandomWorldGenerator` mod embeds massive binary custom structs (`PWEmbeddedWorld`, `PWGenerationParams`) starting with the `PW` prefix. The parser must use the `byte_length` prefix of these structs to skip them cleanly as raw bytes or custom binary blocks without crashing, ensuring full traversal of modded saves.

## 3. Execution Milestones

### Milestone 1: Rust Binary Parser & RWG Extraction (`cargo test`)
1. Initialize a Rust workspace with `byteorder`, `flate2`, `serde`, `serde_json`, `anyhow`, `rayon`, and `axum`.
2. Port the chunk decompressor and actor stream parser.
3. Write an integration test (`cargo test`) that loads files in `./samples/`, decompresses them, parses all actors, and extracts a JSON summary (`rwg_actors.json`) containing:
   - Save metadata (`save_version`, `map_name = ProceduralWorldLevel`).
   - All resource nodes (`BP_ResourceNode_C`, `BP_ResourceDeposit_C`) with their dynamic X, Y, Z transforms.
   - Player structures, drop pods (`BP_DropPod_C`), and player position (`Char_Player_C`).

### Milestone 2: World Snapshot API (`axum`)
1. Implement an `axum` backend server running on `127.0.0.1:8080`.
2. Create an endpoint `GET /api/world` that parses the latest `.sav` file from the `samples/` folder on demand and returns a clean `WorldSnapshot` JSON containing world bounds (`min_x`, `max_x`, `min_y`, `max_y`), resource nodes, and player coordinates.

### Milestone 3: Self-Hosted Interactive Frontend
1. Serve a static HTML/JS page at `http://127.0.0.1:8080/` featuring **Leaflet.js** (using `L.CRS.Simple` for Unreal Engine's flat metric coordinate space).
2. Fetch `/api/world` and dynamically center the map, render color-coded resource node markers, and plot the player's real-time position with hover tooltips.