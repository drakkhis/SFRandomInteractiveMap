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

#[test]
fn test_edit_inventory_slots_roundtrip() {
    let sample_path = Path::new("release/samples/SF Plus Industrial Evolution_201026-222159.sav");
    if !sample_path.exists() {
        eprintln!("skipping test_edit_inventory_slots_roundtrip: sample not found");
        return;
    }

    let raw_bytes = fs::read(sample_path).expect("Failed to read sample save");
    let summary = parse_save_bytes(&raw_bytes).expect("Initial parse failed");

    // Pick an inventory with at least 1 empty slot and 1 occupied slot, or any non-empty inventory
    let target_inv = summary
        .inventories
        .iter()
        .find(|i| !i.items.is_empty() && i.total_slots > i.items.len())
        .or_else(|| summary.inventories.iter().find(|i| !i.items.is_empty()))
        .expect("Expected at least one non-empty inventory in sample save");

    let used: std::collections::HashSet<usize> = target_inv.items.iter().map(|it| it.slot_index).collect();
    let free_slot = (0..target_inv.total_slots).find(|s| !used.contains(s));
    let test_slot = free_slot.unwrap_or(target_inv.items[0].slot_index);
    let test_item = "/Game/FactoryGame/Resource/Parts/IronPlate/Desc_IronPlate.Desc_IronPlate_C";

    let edited_sav = sat_rwg_map::modify_inventory_slots(
        &raw_bytes,
        &target_inv.component_instance_name,
        &[sat_rwg_map::InventorySlotEdit {
            slot_index: test_slot,
            item_path: test_item.to_string(),
            count: 777,
        }],
    )
    .expect("modify_inventory_slots failed");

    let reparsed = parse_save_bytes(&edited_sav).expect("Edited save must re-parse cleanly");
    let edited_inv = reparsed
        .inventories
        .iter()
        .find(|i| i.component_instance_name == target_inv.component_instance_name)
        .expect("Target inventory missing after edit");

    let edited_stack = edited_inv
        .items
        .iter()
        .find(|it| it.slot_index == test_slot)
        .expect("Edited slot missing after roundtrip");

    assert_eq!(edited_stack.item_path, test_item);
    assert_eq!(edited_stack.count, 777);
}

#[test]
fn test_clear_map_fog_roundtrip() {
    let sample_path = Path::new("release/samples/SF Plus Industrial Evolution_201026-222159.sav");
    if !sample_path.exists() {
        eprintln!("skipping test_clear_map_fog_roundtrip: sample not found");
        return;
    }

    let raw_bytes = fs::read(sample_path).expect("Failed to read sample save");
    let initial = parse_save_bytes(&raw_bytes).expect("Initial parse failed");
    assert!(initial.has_fog_of_war, "Expected fog of war in sample save");
    assert!(initial.fog_of_war_png.is_some(), "Expected encoded fog PNG");
    assert!(initial.fog_revealed_pct < 99.0, "Expected partially fogged map initially");

    let cleared_sav = sat_rwg_map::clear_map_fog(&raw_bytes).expect("clear_map_fog failed");
    let reparsed = parse_save_bytes(&cleared_sav).expect("Fog-cleared save must re-parse cleanly");
    assert!(reparsed.has_fog_of_war);
    assert!(
        (reparsed.fog_revealed_pct - 100.0).abs() < 1e-3,
        "Expected 100% revealed after clear_map_fog, got {}",
        reparsed.fog_revealed_pct
    );
}

#[test]
fn test_modify_game_settings_roundtrip() {
    let sample_path = Path::new("release/samples/SF Plus Industrial Evolution_201026-222159.sav");
    if !sample_path.exists() {
        eprintln!("skipping test_modify_game_settings_roundtrip: sample not found");
        return;
    }

    let raw_bytes = fs::read(sample_path).expect("Failed to read sample save");
    let initial = parse_save_bytes(&raw_bytes).expect("Initial parse failed");
    assert!(!initial.game_settings.variables.is_empty(), "Expected game rule variables");

    let target_creative = !initial.game_settings.is_creative_mode_enabled;
    let bool_var = initial
        .game_settings
        .variables
        .iter()
        .find(|v| v.value_type == "bool")
        .expect("Expected at least one bool rule variable");
    let int_var = initial
        .game_settings
        .variables
        .iter()
        .find(|v| v.value_type == "int")
        .expect("Expected at least one int rule variable");

    let new_bool = !bool_var.bool_val.unwrap();
    let new_int = int_var.int_val.unwrap() + 1;

    let updated_sav = sat_rwg_map::modify_game_settings(
        &raw_bytes,
        Some(target_creative),
        &[
            sat_rwg_map::GameRuleUpdate {
                actor_instance_name: bool_var.actor_instance_name.clone(),
                property_name: bool_var.property_name.clone(),
                value_type: None,
                bool_val: Some(new_bool),
                int_val: None,
                float_val: None,
            },
            sat_rwg_map::GameRuleUpdate {
                actor_instance_name: int_var.actor_instance_name.clone(),
                property_name: int_var.property_name.clone(),
                value_type: None,
                bool_val: None,
                int_val: Some(new_int),
                float_val: None,
            },
        ],
    )
    .expect("modify_game_settings failed");

    let reparsed = parse_save_bytes(&updated_sav).expect("Updated save must re-parse cleanly");
    assert_eq!(reparsed.game_settings.is_creative_mode_enabled, target_creative);

    let reparsed_bool = reparsed
        .game_settings
        .variables
        .iter()
        .find(|v| v.actor_instance_name == bool_var.actor_instance_name && v.property_name == bool_var.property_name)
        .unwrap();
    assert_eq!(reparsed_bool.bool_val, Some(new_bool));

    let reparsed_int = reparsed
        .game_settings
        .variables
        .iter()
        .find(|v| v.actor_instance_name == int_var.actor_instance_name && v.property_name == int_var.property_name)
        .unwrap();
    assert_eq!(reparsed_int.int_val, Some(new_int));
}

#[test]
fn test_satisfactory_12_min_settings_insertion_roundtrip() {
    let sample_path = std::path::PathBuf::from("samples/Min Settings_131126-204314.sav");
    if !sample_path.exists() {
        return;
    }

    let raw_bytes = std::fs::read(&sample_path).expect("Failed to read Min Settings sample");
    let initial = parse_save_bytes(&raw_bytes).expect("Initial parse failed");

    let no_power = initial
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mCheatNoPower")
        .expect("mCheatNoPower preset must be present");
    let starting_tier = initial
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mStartingTier")
        .expect("mStartingTier preset must be present");
    let energy_mult = initial
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mEnergyCostMultiplier")
        .expect("mEnergyCostMultiplier preset must be present");

    assert_eq!(no_power.bool_val, Some(false));
    assert_eq!(starting_tier.int_val, Some(0));
    assert!((energy_mult.float_val.unwrap() - 0.25).abs() < 1e-4);

    let updated_sav = sat_rwg_map::modify_game_settings(
        &raw_bytes,
        Some(true),
        &[
            sat_rwg_map::GameRuleUpdate {
                actor_instance_name: no_power.actor_instance_name.clone(),
                property_name: "mCheatNoPower".to_string(),
                value_type: Some("bool".to_string()),
                bool_val: Some(true),
                int_val: None,
                float_val: None,
            },
            sat_rwg_map::GameRuleUpdate {
                actor_instance_name: starting_tier.actor_instance_name.clone(),
                property_name: "mStartingTier".to_string(),
                value_type: Some("int".to_string()),
                bool_val: None,
                int_val: Some(5),
                float_val: None,
            },
            sat_rwg_map::GameRuleUpdate {
                actor_instance_name: energy_mult.actor_instance_name.clone(),
                property_name: "mEnergyCostMultiplier".to_string(),
                value_type: Some("float".to_string()),
                bool_val: None,
                int_val: None,
                float_val: Some(2.5),
                },
        ],
    )
    .expect("modify_game_settings insertion failed");

    let reparsed = parse_save_bytes(&updated_sav).expect("Re-parse after insertion failed");
    assert!(reparsed.game_settings.is_creative_mode_enabled);

    let r_no_power = reparsed
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mCheatNoPower")
        .unwrap();
    assert_eq!(r_no_power.bool_val, Some(true));

    let r_tier = reparsed
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mStartingTier")
        .unwrap();
    assert_eq!(r_tier.int_val, Some(5));

    let r_energy = reparsed
        .game_settings
        .variables
        .iter()
        .find(|v| v.property_name == "mEnergyCostMultiplier")
        .unwrap();
    assert!((r_energy.float_val.unwrap() - 2.5).abs() < 1e-4);
}

#[test]
fn test_duplicate_delete_and_blueprint_export_roundtrip() {
    let sample_candidates = [
        "release/samples/Random World Plus_071126-124858.sav",
        "release/samples/Draconia Plus_221026-210844.sav",
        "release/samples/Random 2.0 SF+ Dev_261026-194354.sav",
        "Referenc_specs/RP Test_autosave_1.sav",
        "release/samples/Random 0.2.0_121126-231201.sav",
    ];
    let (raw_bytes, initial) = sample_candidates
        .iter()
        .map(Path::new)
        .filter(|p| p.exists())
        .find_map(|p| {
            let bytes = fs::read(p).ok()?;
            let parsed = parse_save_bytes(&bytes).ok()?;
            if parsed.player_structures.is_empty() {
                None
            } else {
                Some((bytes, parsed))
            }
        })
        .expect("At least one sample save with player structures must exist");

    let mut targets: Vec<String> = Vec::new();
    let initial_count = initial.player_structures.len();

    for s in &initial.player_structures {
        if !s.is_lightweight
            && targets.iter().filter(|t| !t.contains("::lw::")).count() < 2
            && !s.type_path.contains("Subsystem")
        {
            targets.push(s.instance_name.clone());
        } else if s.is_lightweight && targets.iter().filter(|t| t.contains("::lw::")).count() < 2 {
            targets.push(s.instance_name.clone());
        }
        if targets.len() >= 4 {
            break;
        }
    }

    assert!(
        !targets.is_empty(),
        "Sample save should contain at least one player structure or lightweight buildable"
    );

    // 1. Duplicate selected structures
    let (dup_sav, dup_count) = sat_rwg_map::duplicate_structures_in_save(
        &raw_bytes,
        &targets,
        800.0,
        0.0,
        0.0,
    )
    .expect("duplicate_structures_in_save failed");
    assert_eq!(dup_count, targets.len());

    let after_dup = parse_save_bytes(&dup_sav).expect("Re-parse after duplicate failed");
    assert_eq!(
        after_dup.player_structures.len(),
        initial_count + dup_count,
        "Total structures should increase by duplicated count"
    );

    // 2. Delete the original target structures from the duplicated save
    let (del_sav, del_count) = sat_rwg_map::delete_structures_in_save(&dup_sav, &targets)
        .expect("delete_structures_in_save failed");
    assert_eq!(del_count, targets.len());

    let after_del = parse_save_bytes(&del_sav).expect("Re-parse after delete failed");
    assert_eq!(
        after_del.player_structures.len(),
        initial_count,
        "Total structures should return to initial count after deleting targets"
    );

    // 3. Export selection as blueprint (.sbp + .sbpcfg)
    let (sbp_bytes, sbpcfg_bytes, bp_count) =
        sat_rwg_map::export_selection_as_blueprint(&raw_bytes, &targets, "Test Blueprint")
            .expect("export_selection_as_blueprint failed");
    assert_eq!(bp_count, targets.len());
    assert_eq!(&sbp_bytes[0..4], &2u32.to_le_bytes());
    assert_eq!(&sbpcfg_bytes[0..4], &6u32.to_le_bytes());
    assert!(sbp_bytes.len() > 64);
    assert!(sbpcfg_bytes.len() > 24);

    // Verify .sbp payload structure: [total_m4][hdr_len][hdr_cnt][headers...][obj_len][obj_cnt]([body_len][body]...)
    let chunk_pos = sbp_bytes
        .windows(4)
        .position(|w| w == [0xC1, 0x83, 0x2A, 0x9E])
        .expect("Compressed chunk tag missing in .sbp");
    let bp_decomp = decompress_save_chunks(&sbp_bytes, chunk_pos)
        .expect("Failed to decompress .sbp chunks")
        .uncompressed_data;
    let total_m4 = u32::from_le_bytes(bp_decomp[0..4].try_into().unwrap()) as usize;
    assert_eq!(total_m4 + 4, bp_decomp.len());
    let hdr_len = u32::from_le_bytes(bp_decomp[4..8].try_into().unwrap()) as usize;
    let obj_off = 8 + hdr_len;
    let obj_cnt = u32::from_le_bytes(bp_decomp[obj_off + 4..obj_off + 8].try_into().unwrap()) as usize;
    let mut pos = obj_off + 8;
    for _ in 0..obj_cnt {
        let bsz = u32::from_le_bytes(bp_decomp[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4 + bsz;
    }
    assert_eq!(pos, bp_decomp.len(), ".sbp object bodies must walk cleanly to EOF without .sav 8-byte prefix");
}


