use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use sat_rwg_map::{create_app, AppState, WorldSnapshot};
use std::path::PathBuf;
use tower::ServiceExt; // for oneshot

#[tokio::test]
async fn test_api_world_endpoint() {
    let state = AppState::new(PathBuf::from("samples"));
    let app = create_app(state);

    // 1. Test GET / (HTML frontend)
    let index_req = Request::builder()
        .uri("/")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let index_res = app.clone().oneshot(index_req).await.unwrap();
    assert_eq!(index_res.status(), StatusCode::OK);
    let index_bytes = axum::body::to_bytes(index_res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let index_html = String::from_utf8_lossy(&index_bytes);
    assert!(index_html.contains("Satisfactory RWG Interactive Map"));
    assert!(index_html.contains("L.CRS.Simple"));

    // 2. Test GET /api/status
    let status_req = Request::builder()
        .uri("/api/status")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let status_res = app.clone().oneshot(status_req).await.unwrap();
    assert_eq!(status_res.status(), StatusCode::OK);

    // 2b. Test GET /static/landmass.png (Textured terrain layer)
    let landmass_req = Request::builder()
        .uri("/static/landmass.png")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let landmass_res = app.clone().oneshot(landmass_req).await.unwrap();
    assert_eq!(landmass_res.status(), StatusCode::OK);
    assert_eq!(
        landmass_res.headers().get("content-type").unwrap(),
        "image/png"
    );
    let landmass_bytes = axum::body::to_bytes(landmass_res.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    assert!(landmass_bytes.len() > 100_000);
    // Validate PNG signature: \x89PNG\r\n\x1a\n
    assert_eq!(&landmass_bytes[..8], b"\x89PNG\r\n\x1a\n");

    // 2c. Test GET /api/saves
    let saves_req = Request::builder()
        .uri("/api/saves")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let saves_res = app.clone().oneshot(saves_req).await.unwrap();
    assert_eq!(saves_res.status(), StatusCode::OK);
    let saves_bytes = axum::body::to_bytes(saves_res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let saves: Vec<sat_rwg_map::server::SaveEntry> = serde_json::from_slice(&saves_bytes).unwrap();
    assert!(!saves.is_empty(), "Expected at least one save file in samples/");
    assert!(saves.iter().any(|s| s.filename.ends_with(".sav")));

    // 2d. Test POST /api/load explicitly with tera_autosave_0.sav
    let load_req = Request::builder()
        .uri("/api/load?filename=tera_autosave_0.sav")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let load_res = app.clone().oneshot(load_req).await.unwrap();
    assert_eq!(load_res.status(), StatusCode::OK);

    // Verify GET /api/map.png returns the dynamically generated PNG for tera_autosave_0
    let map_req_1 = Request::builder()
        .uri("/api/map.png")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let map_res_1 = app.clone().oneshot(map_req_1).await.unwrap();
    assert_eq!(map_res_1.status(), StatusCode::OK);
    let map_bytes_1 = axum::body::to_bytes(map_res_1.into_body(), 15 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(&map_bytes_1[..8], b"\x89PNG\r\n\x1a\n");
    assert!(map_bytes_1.len() > 100_000, "Generated PNG should be non-trivial");

    // Verify GET /api/heightmap.bin returns the 512x512 PWHM binary heightmap
    let hm_req = Request::builder()
        .uri("/api/heightmap.bin")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let hm_res = app.clone().oneshot(hm_req).await.unwrap();
    assert_eq!(hm_res.status(), StatusCode::OK);
    let hm_bytes = axum::body::to_bytes(hm_res.into_body(), 5 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(&hm_bytes[..4], b"PWHM");
    assert_eq!(hm_bytes.len(), 28 + 512 * 512 * 2);

    // 3. Test GET /api/world
    println!("Requesting GET /api/world...");
    let world_req = Request::builder()
        .uri("/api/world")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let world_res = app.clone().oneshot(world_req).await.unwrap();
    assert_eq!(world_res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(world_res.into_body(), 100 * 1024 * 1024)
        .await
        .expect("Failed to read response body");

    let snapshot: WorldSnapshot = serde_json::from_slice(&body_bytes)
        .expect("Failed to deserialize WorldSnapshot JSON");

    println!("=== Received WorldSnapshot ===");
    println!("Session Name     : {}", snapshot.session_name);
    println!("Map Name         : {}", snapshot.map_name);
    println!("World Seed       : {:?}", snapshot.world_seed);
    println!(
        "World Bounds     : X: [{:.1}, {:.1}], Y: [{:.1}, {:.1}], Z: [{:.1}, {:.1}]",
        snapshot.world_bounds.min_x,
        snapshot.world_bounds.max_x,
        snapshot.world_bounds.min_y,
        snapshot.world_bounds.max_y,
        snapshot.world_bounds.min_z,
        snapshot.world_bounds.max_z
    );
    println!("Resource Nodes   : {}", snapshot.total_nodes);
    println!("Resource Deposits: {}", snapshot.resource_deposits.len());
    println!("Drop Pods        : {}", snapshot.total_drop_pods);
    if let Some(ref p) = snapshot.player {
        println!(
            "Player Position  : [{:.1}, {:.1}, {:.1}]",
            p.transform.translation[0],
            p.transform.translation[1],
            p.transform.translation[2]
        );
    }

    // Verify assertions from Milestone 2 specifications
    assert_eq!(snapshot.session_name, "tera");
    assert_eq!(snapshot.map_name, "ProceduralWorldLevel");
    assert_eq!(snapshot.world_seed, Some(8675309));
    assert_eq!(snapshot.world_size_meters, Some(12000.0));
    assert!(snapshot.world_bounds.min_x < -500000.0);
    assert!(snapshot.world_bounds.max_x > 500000.0);
    assert!(snapshot.world_bounds.min_y < -500000.0);
    assert!(snapshot.world_bounds.max_y > 500000.0);
    assert!(snapshot.total_nodes > 2000);
    assert!(snapshot.resource_deposits.len() > 5000);
    assert!(snapshot.total_drop_pods > 200);
    assert!(snapshot.total_somersloops > 100);
    assert!(snapshot.total_mercer_spheres > 200);
    assert!(snapshot.player.is_some());

    // 4. Switch save to Map Test_191026-233059.sav and verify the generated map PNG updates!
    if saves.iter().any(|s| s.filename == "Map Test_191026-233059.sav") {
        let load_req_2 = Request::builder()
            .uri("/api/load?filename=Map%20Test_191026-233059.sav")
            .method("POST")
            .body(Body::empty())
            .unwrap();
        let load_res_2 = app.clone().oneshot(load_req_2).await.unwrap();
        assert_eq!(load_res_2.status(), StatusCode::OK);

        let map_req_2 = Request::builder()
            .uri("/api/map.png")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let map_res_2 = app.clone().oneshot(map_req_2).await.unwrap();
        assert_eq!(map_res_2.status(), StatusCode::OK);
        let map_bytes_2 = axum::body::to_bytes(map_res_2.into_body(), 15 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(&map_bytes_2[..8], b"\x89PNG\r\n\x1a\n");
        assert_ne!(
            map_bytes_1, map_bytes_2,
            "Dynamically generated map PNG must change when loading a different RWG save!"
        );
        std::fs::write("scratch/rust_generated_maptest.png", &map_bytes_2).unwrap();
        std::fs::write("scratch/rust_generated_rptest.png", &map_bytes_1).unwrap();
    }

    // 5. Test POST /api/save/teleport
    let teleport_payload = serde_json::json!({
        "x": 99999.0,
        "y": -88888.0,
        "z": 7777.0
    });
    let teleport_req = Request::builder()
        .uri("/api/save/teleport")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(teleport_payload.to_string()))
        .unwrap();

    let teleport_res = app.clone().oneshot(teleport_req).await.unwrap();
    assert_eq!(teleport_res.status(), StatusCode::OK);
    assert_eq!(
        teleport_res.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(teleport_res.headers().contains_key("content-disposition"));

    let teleport_bytes = axum::body::to_bytes(teleport_res.into_body(), 100 * 1024 * 1024)
        .await
        .unwrap();

    let repacked_summary = sat_rwg_map::parse_save_bytes(&teleport_bytes)
        .expect("Teleported binary must parse as valid save file");
    let repacked_player = repacked_summary.player.expect("Player must exist");
    assert_eq!(repacked_player.transform.translation, [99999.0, -88888.0, 7777.0]);

    // 6. Test POST /api/player/move (staged move without downloading stream)
    let move_player_req = Request::builder()
        .uri("/api/player/move")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({
            "x": 11111.0,
            "y": 22222.0,
            "z": 3333.0
        }).to_string()))
        .unwrap();

    let move_player_res = app.clone().oneshot(move_player_req).await.unwrap();
    assert_eq!(move_player_res.status(), StatusCode::OK);

    // 7. Test POST /api/node/move
    let first_node_name = snapshot.resource_nodes[0].instance_name.clone();
    let move_node_req = Request::builder()
        .uri("/api/node/move")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({
            "instance_name": first_node_name,
            "x": 44444.0,
            "y": -55555.0,
            "z": 6666.0
        }).to_string()))
        .unwrap();

    let move_node_res = app.clone().oneshot(move_node_req).await.unwrap();
    assert_eq!(move_node_res.status(), StatusCode::OK);

    // 8. Test GET /api/save/download
    let dl_req = Request::builder()
        .uri("/api/save/download")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let dl_res = app.clone().oneshot(dl_req).await.unwrap();
    assert_eq!(dl_res.status(), StatusCode::OK);
    assert_eq!(
        dl_res.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(dl_res.headers().contains_key("content-disposition"));
}

