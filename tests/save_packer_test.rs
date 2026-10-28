use std::fs;
use std::path::Path;
use sat_rwg_map::{
    decompress_save_chunks, modify_node_coordinates, modify_player_coordinates, parse_save_bytes,
    parse_save_header, recompress_save,
};

#[test]
fn test_recompress_and_teleport_player() {
    let sample_candidates = [
        "samples/SF+ Random World 5_281026-182315.sav",
        "samples/tera_autosave_0.sav",
        "samples/RP Test_autosave_1.sav",
        "samples/Map Test_191026-233059.sav",
        "Referenc_specs/RP Test_autosave_1.sav",
    ];
    let save_path = sample_candidates
        .iter()
        .map(Path::new)
        .find(|p| p.exists())
        .expect("At least one sample save must exist in samples/");

    let raw_bytes = fs::read(save_path).expect("Failed to read sample save");
    let (header, chunk_offset) = parse_save_header(&raw_bytes).expect("Failed to parse save header");
    let decompressed = decompress_save_chunks(&raw_bytes, chunk_offset).expect("Failed to decompress chunks");

    // 1. Verify round-trip recompression of unmodified data
    let header_bytes = &raw_bytes[0..chunk_offset];
    let repacked = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .expect("Failed to recompress save");

    let (re_header, re_chunk_offset) = parse_save_header(&repacked).expect("Failed to parse repacked header");
    assert_eq!(re_chunk_offset, chunk_offset);
    assert_eq!(re_header.session_name, header.session_name);

    let re_decompressed = decompress_save_chunks(&repacked, re_chunk_offset)
        .expect("Failed to decompress repacked chunks");
    assert_eq!(
        decompressed.uncompressed_data,
        re_decompressed.uncompressed_data,
        "Decompressed data from repacked save must be byte-for-byte identical"
    );

    // 2. Modify player coordinates
    let target_x = 12345.0f32;
    let target_y = -67890.0f32;
    let target_z = 4242.0f32;

    let (modified_sav, modified_players) = modify_player_coordinates(&raw_bytes, target_x, target_y, target_z)
        .expect("Failed to modify player coordinates");

    assert!(!modified_players.is_empty(), "Should have modified at least one player");
    assert_eq!(modified_players[0].transform.translation, [target_x, target_y, target_z]);

    // 3. Re-parse the modified .sav binary from scratch using parse_save_bytes
    let re_parsed_summary = parse_save_bytes(&modified_sav)
        .expect("Failed to parse newly generated modified .sav file");

    let new_player = re_parsed_summary.player.expect("Player must be present in parsed save");
    println!("Teleported player position: {:?}", new_player.transform.translation);

    assert_eq!(new_player.transform.translation[0], target_x);
    assert_eq!(new_player.transform.translation[1], target_y);
    assert_eq!(new_player.transform.translation[2], target_z);
}

#[test]
fn test_teleport_on_water_dams_save() {
    let save_path = Path::new("samples/watter dams_221026-105432.sav");
    if !save_path.exists() {
        println!("Water dams save not found in samples, skipping");
        return;
    }

    let raw_bytes = fs::read(save_path).expect("Failed to read water dams save");
    let target_x = 55555.0f32;
    let target_y = -44444.0f32;
    let target_z = 3333.0f32;

    let (modified_sav, _) = modify_player_coordinates(&raw_bytes, target_x, target_y, target_z)
        .expect("Failed to modify coordinates on water dams save");

    let re_parsed = parse_save_bytes(&modified_sav).expect("Failed to parse water dams modified save");
    let player = re_parsed.player.expect("Player must be present");
    assert_eq!(player.transform.translation, [target_x, target_y, target_z]);
}

#[test]
fn test_move_fracking_core_and_satellites() {
    let save_path = Path::new("samples/watter dams_221026-105432.sav");
    if !save_path.exists() {
        return;
    }

    let raw_bytes = fs::read(save_path).expect("Failed to read save");
    let initial_summary = parse_save_bytes(&raw_bytes).expect("Failed to parse initial save");

    // Locate BP_FrackingCore5 in water dams save
    let core = initial_summary
        .resource_nodes
        .iter()
        .find(|n| n.instance_name.contains("BP_FrackingCore5"))
        .expect("BP_FrackingCore5 must exist in water dams save");

    let old_core_pos = core.transform.translation;

    // Find initial satellites within 90m
    let init_sats: Vec<_> = initial_summary
        .resource_nodes
        .iter()
        .filter(|n| {
            if !n.instance_name.contains("BP_FrackingSatellite") {
                return false;
            }
            let d_sq = (n.transform.translation[0] - old_core_pos[0]).powi(2)
                + (n.transform.translation[1] - old_core_pos[1]).powi(2);
            d_sq <= 9000.0 * 9000.0
        })
        .cloned()
        .collect();

    assert!(!init_sats.is_empty(), "Core5 must have satellites nearby");
    println!("Found {} satellites associated with BP_FrackingCore5", init_sats.len());

    // Move Core by +10000 cm X, -5000 cm Y, +200 cm Z
    let new_core_x = old_core_pos[0] + 10000.0;
    let new_core_y = old_core_pos[1] - 5000.0;
    let new_core_z = old_core_pos[2] + 200.0;

    let (modified_sav, moved_nodes) = modify_node_coordinates(
        &raw_bytes,
        &core.instance_name,
        new_core_x,
        new_core_y,
        new_core_z,
        None,
    )
    .expect("Failed to move core");

    assert_eq!(moved_nodes.len(), 1 + init_sats.len(), "Core + all its satellites should be moved");

    // Verify by parsing the newly generated .sav from scratch
    let repacked_summary = parse_save_bytes(&modified_sav)
        .expect("Modified save must parse cleanly");

    let repacked_core = repacked_summary
        .resource_nodes
        .iter()
        .find(|n| n.instance_name.contains("BP_FrackingCore5"))
        .unwrap();

    assert_eq!(repacked_core.transform.translation[0], new_core_x);
    assert_eq!(repacked_core.transform.translation[1], new_core_y);
    assert_eq!(repacked_core.transform.translation[2], new_core_z);

    // Verify each satellite moved by the exact same delta (+10000, -5000, +200)
    for orig_sat in &init_sats {
        let repacked_sat = repacked_summary
            .resource_nodes
            .iter()
            .find(|n| n.instance_name == orig_sat.instance_name)
            .unwrap();

        assert_eq!(repacked_sat.transform.translation[0], orig_sat.transform.translation[0] + 10000.0);
        assert_eq!(repacked_sat.transform.translation[1], orig_sat.transform.translation[1] - 5000.0);
        assert_eq!(repacked_sat.transform.translation[2], orig_sat.transform.translation[2] + 200.0);
    }

    // Now test moving with a ground slope rotation (e.g. 15-degree pitch incline)
    // Pitch ~15 deg around Y: rot_y = sin(7.5 deg) ~ 0.130526, rot_w = cos(7.5 deg) ~ 0.991445
    let slope_rot = [0.0f32, 0.130526f32, 0.0f32, 0.991445f32];
    let (slope_sav, slope_moved) = modify_node_coordinates(
        &raw_bytes,
        &core.instance_name,
        new_core_x + 500.0,
        new_core_y + 500.0,
        new_core_z + 100.0,
        Some(slope_rot),
    )
    .expect("Failed to move core with slope rotation");

    assert_eq!(slope_moved.len(), 1 + init_sats.len());

    let slope_summary = parse_save_bytes(&slope_sav).expect("Slope-modified save must parse cleanly");
    let repacked_slope_core = slope_summary
        .resource_nodes
        .iter()
        .find(|n| n.instance_name.contains("BP_FrackingCore5"))
        .unwrap();

    assert!((repacked_slope_core.transform.rotation[1] - slope_rot[1]).abs() < 1e-4);
    assert!((repacked_slope_core.transform.rotation[3] - slope_rot[3]).abs() < 1e-4);
}

