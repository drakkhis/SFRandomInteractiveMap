use anyhow::{Context, Result};
use sat_rwg_map::{create_app, find_latest_save, parse_save_file, AppState, RwgActorsSummary};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();

    // Check if CLI mode was requested
    if args.iter().any(|arg| arg == "--cli" || arg == "-c") {
        let save_path = if args.len() > 2 {
            PathBuf::from(&args[2])
        } else {
            find_latest_save(Path::new("samples"))
                .context("Could not find any save file in samples/")?
        };

        println!("=======================================================");
        println!(" Satisfactory RWG Map & Save Tool (CLI Mode)");
        println!("=======================================================");
        println!("Parsing save: {:?}", save_path);
        let start_time = Instant::now();

        let summary: RwgActorsSummary = parse_save_file(&save_path)
            .with_context(|| format!("Failed to parse save file {:?}", save_path))?;

        let duration = start_time.elapsed();
        println!("Parsed in {:.2}s!", duration.as_secs_f64());
        println!("-------------------------------------------------------");
        println!("Session Name       : {}", summary.save_metadata.session_name);
        println!("Map Name           : {}", summary.save_metadata.map_name);
        println!("Save Version       : {}", summary.save_metadata.save_version);
        println!("Build Version      : {}", summary.save_metadata.build_version);
        println!("World Seed         : {:?}", summary.world_seed);
        println!("Total Actors       : {}", summary.total_actor_headers);
        println!("Total Objects      : {}", summary.total_objects_parsed);
        println!("Resource Nodes     : {}", summary.resource_nodes.len());
        println!("Resource Deposits  : {}", summary.resource_deposits.len());
        println!("Drop Pods          : {}", summary.drop_pods.len());
        if let Some(ref player) = summary.player {
            println!(
                "Player Position    : [{:.1}, {:.1}, {:.1}]",
                player.transform.translation[0],
                player.transform.translation[1],
                player.transform.translation[2]
            );
        }
        println!("-------------------------------------------------------");

        let output_path = Path::new("rwg_actors.json");
        let json_str = serde_json::to_string_pretty(&summary)?;
        fs::write(output_path, json_str)?;
        println!("Extracted JSON summary written to {:?}", output_path);
        return Ok(());
    }

    // Default mode: Axum Web Server (Milestone 2)
    let samples_dir = PathBuf::from("samples");
    let state = AppState::new(&samples_dir);
    let app = create_app(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    println!("=======================================================");
    println!(" Satisfactory RWG Map & Save Server (sat-rwg-map)");
    println!("=======================================================");
    println!("Serving on http://{}", addr);
    println!("Endpoints:");
    println!("  GET http://{}/api/status", addr);
    println!("  GET http://{}/api/world", addr);
    println!("=======================================================");

    let listener = tokio::net::TcpListener::bind(addr).await
        .with_context(|| format!("Failed to bind server to {}", addr))?;

    axum::serve(listener, app).await
        .context("Server execution error")?;

    Ok(())
}
