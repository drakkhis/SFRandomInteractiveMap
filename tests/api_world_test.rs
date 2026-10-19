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

    // 3. Test GET /api/world
    println!("Requesting GET /api/world...");
    let world_req = Request::builder()
        .uri("/api/world")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let world_res = app.oneshot(world_req).await.unwrap();
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
    assert_eq!(snapshot.session_name, "RP Test");
    assert_eq!(snapshot.map_name, "ProceduralWorldLevel");
    assert_eq!(snapshot.world_seed, Some(8675309));
    assert_eq!(snapshot.world_size_meters, Some(12000.0));
    assert!(snapshot.world_bounds.min_x < -500000.0);
    assert!(snapshot.world_bounds.max_x > 500000.0);
    assert!(snapshot.world_bounds.min_y < -500000.0);
    assert!(snapshot.world_bounds.max_y > 500000.0);
    assert_eq!(snapshot.total_nodes, 2550); // 1834 standard + 97 geysers + 78 fracking cores + 540 fracking satellites + 1
    assert_eq!(snapshot.resource_deposits.len(), 8341);
    assert_eq!(snapshot.total_drop_pods, 289);
    assert_eq!(snapshot.total_somersloops, 261);
    assert_eq!(snapshot.total_mercer_spheres, 733);
    assert!(snapshot.player.is_some());
}

