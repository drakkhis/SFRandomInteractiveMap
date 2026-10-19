use sat_rwg_map::parse_save_file;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[test]
fn test_parse_sample_save() {
    let sample_path = Path::new("samples/RP Test_autosave_1.sav");
    assert!(
        sample_path.exists(),
        "Sample file {:?} must exist for testing",
        sample_path
    );

    println!("Starting parse of sample save: {:?}", sample_path);
    let start_time = Instant::now();

    let summary = parse_save_file(sample_path)
        .expect("Failed to parse sample save file");

    let duration = start_time.elapsed();
    println!("Parsed successfully in {:.2} seconds!", duration.as_secs_f64());

    // 1. Verify Save Metadata
    println!("=== Save Metadata ===");
    println!("Save Header Type: {}", summary.save_metadata.save_header_type);
    println!("Save Version: {}", summary.save_metadata.save_version);
    println!("Build Version: {}", summary.save_metadata.build_version);
    println!("Map Name: {}", summary.save_metadata.map_name);
    println!("Session Name: {}", summary.save_metadata.session_name);
    println!("Play Duration: {}s", summary.save_metadata.play_duration_seconds);
    println!("Is Modded: {}", summary.save_metadata.is_modded_save);

    assert_eq!(summary.save_metadata.save_header_type, 14);
    assert_eq!(summary.save_metadata.save_version, 60);
    assert_eq!(summary.save_metadata.map_name, "ProceduralWorldLevel");
    assert_eq!(summary.save_metadata.session_name, "RP Test");

    // 2. Verify World Generator data
    println!("World Seed: {:?}", summary.world_seed);
    assert_eq!(summary.world_seed, Some(8675309));
    println!("World Size Meters: {:?}", summary.world_size_meters);
    assert_eq!(summary.world_size_meters, Some(12000.0));

    // 3. Verify Player position
    let player = summary.player.as_ref().expect("Player must be present");
    println!(
        "Player instance: {}, position: {:?}",
        player.instance_name, player.transform.translation
    );
    assert!((player.transform.translation[0] - (-206399.19)).abs() < 50.0);
    assert!((player.transform.translation[1] - (-22322.84)).abs() < 50.0);

    // 4. Verify Resource Nodes and Deposits
    println!("Total Actor Headers: {}", summary.total_actor_headers);
    println!("Total Objects Parsed: {}", summary.total_objects_parsed);
    println!("Resource Nodes: {}", summary.resource_nodes.len());
    println!("Resource Deposits: {}", summary.resource_deposits.len());
    println!("Drop Pods: {}", summary.drop_pods.len());
    println!("Player Structures: {}", summary.player_structures.len());
    println!("Somersloops: {}", summary.somersloops.len());
    println!("Mercer Spheres: {}", summary.mercer_spheres.len());

    assert!(summary.resource_nodes.len() > 1000, "Expected > 1000 resource nodes");
    assert!(summary.resource_deposits.len() > 5000, "Expected > 5000 resource deposits");
    assert!(summary.drop_pods.len() > 200, "Expected > 200 drop pods");
    assert_eq!(summary.somersloops.len(), 261, "Expected 261 Somersloops (BP_WAT1)");
    assert_eq!(summary.mercer_spheres.len(), 733, "Expected 733 Mercer Spheres (BP_WAT2)");

    // Verify Refined Power nodes are identified
    let rp_nodes: Vec<_> = summary
        .resource_nodes
        .iter()
        .filter(|n| n.mod_source.as_deref() == Some("RefinedPower"))
        .collect();
    println!("Refined Power nodes detected: {}", rp_nodes.len());
    assert!(!rp_nodes.is_empty(), "Expected Refined Power nodes to be detected");

    // Verify some sample node data
    let sample_node = &summary.resource_nodes[0];
    println!("Sample Node: {:?}", sample_node);
    assert!(!sample_node.instance_name.is_empty());

    // 5. Output rwg_actors.json
    let json_output_path = Path::new("rwg_actors.json");
    let json_data = serde_json::to_string_pretty(&summary)
        .expect("Failed to serialize summary to JSON");
    fs::write(json_output_path, json_data)
        .expect("Failed to write rwg_actors.json");

    println!(
        "Summary written to {:?} ({} bytes)",
        json_output_path,
        fs::metadata(json_output_path).unwrap().len()
    );
}

#[test]
fn test_ficsit_farming_and_mod_recognition() {
    use sat_rwg_map::{detect_mod_source, extract_clean_name};

    // Test Ficsit Farming soils
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_Loam.Desc_Loam_C"), "Loam");
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_Peat.Desc_Peat_C"), "Peat");
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_PeatSoil.Desc_PeatSoil_C"), "Peat");
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_Dirt.Desc_Dirt_C"), "Dirt");
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_FF_Dirt.Desc_FF_Dirt_C"), "Dirt");

    // Test Algie Mass / Algal Mass
    assert_eq!(extract_clean_name("/SatisfactoryPlus/Resource/Desc_AlgalMass.Desc_AlgalMass_C"), "Algie Mass");
    assert_eq!(extract_clean_name("/SatisfactoryPlus/Resource/Desc_AlgieMass.Desc_AlgieMass_C"), "Algie Mass");
    assert_eq!(extract_clean_name("/FicsitFarming/Resource/Desc_AlgaeMass.Desc_AlgaeMass_C"), "Algie Mass");

    // Test Mod Source Detection
    assert_eq!(
        detect_mod_source(
            "/Game/FactoryGame/Resource/BP_ResourceNode.BP_ResourceNode_C",
            Some("/FicsitFarming/Resource/Desc_Loam.Desc_Loam_C")
        ),
        "FicsitFarming"
    );
    assert_eq!(
        detect_mod_source(
            "/Game/FactoryGame/Resource/BP_ResourceNode.BP_ResourceNode_C",
            Some("/FicsitFarming/Resource/Desc_Peat.Desc_Peat_C")
        ),
        "FicsitFarming"
    );
    assert_eq!(
        detect_mod_source(
            "/Game/FactoryGame/Resource/BP_ResourceNode.BP_ResourceNode_C",
            Some("/FicsitFarming/Resource/Desc_Dirt.Desc_Dirt_C")
        ),
        "FicsitFarming"
    );
    assert_eq!(
        detect_mod_source(
            "/Game/FactoryGame/Resource/BP_ResourceNode.BP_ResourceNode_C",
            Some("/SatisfactoryPlus/Resource/Desc_AlgalMass.Desc_AlgalMass_C")
        ),
        "SatisfactoryPlus"
    );
    assert_eq!(
        detect_mod_source(
            "/Game/FactoryGame/Resource/BP_ResourceNode.BP_ResourceNode_C",
            Some("/RefinedPower/World/ResourceNodes/Thorium/Desc_RP_Thorium.Desc_RP_Thorium_C")
        ),
        "RefinedPower"
    );
}



