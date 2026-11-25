#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{bail, Context, Result};
use sat_rwg_map::{
    create_app, find_latest_save, parse_save_file, platform, AppState, RwgActorsSummary,
};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default)]
struct CliOptions {
    help: bool,
    version: bool,
    cli: bool,
    server: bool,
    browser: bool,
    port: Option<u16>,
    save_path: Option<PathBuf>,
}

fn parse_cli_options() -> Result<CliOptions> {
    let mut opts = CliOptions::default();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => opts.help = true,
            "-V" | "--version" => opts.version = true,
            "-c" | "--cli" => opts.cli = true,
            "--server" => opts.server = true,
            "--browser" => opts.browser = true,
            "--port" => {
                let raw = args
                    .next()
                    .context("--port requires a numeric port argument")?;
                opts.port = Some(
                    raw.parse::<u16>()
                        .with_context(|| format!("Invalid port value: {}", raw))?,
                );
            }
            _ if arg.starts_with("--port=") => {
                let raw = &arg["--port=".len()..];
                opts.port = Some(
                    raw.parse::<u16>()
                        .with_context(|| format!("Invalid port value: {}", raw))?,
                );
            }
            _ if arg.starts_with('-') => {
                bail!("Unknown option '{}'. Run with --help for usage.", arg);
            }
            _ => {
                if opts.save_path.is_none() {
                    opts.save_path = Some(PathBuf::from(arg));
                } else {
                    bail!(
                        "Unexpected extra positional argument '{}'. Run with --help for usage.",
                        arg
                    );
                }
            }
        }
    }

    Ok(opts)
}

fn print_help() {
    println!(
        "Satisfactory RWG Interactive Map (sat-rwg-map v{})

USAGE:
  sat-rwg-map [OPTIONS]
  sat-rwg-map --cli [SAVE_FILE]

OPTIONS:
  (default)          Launch native desktop window (or server when built headless)
  --server           Run headless HTTP server only (default port 8080)
  --browser          Start HTTP server and open UI in your default web browser
  --port <PORT>      Override local HTTP server port (default: 8080)
  -c, --cli [FILE]   Parse a .sav file and write rwg_actors.json in current directory
  -V, --version      Print version information
  -h, --help         Show this help message

ENVIRONMENT VARIABLES:
  SATISFACTORY_SAVE_DIR          Override Satisfactory SaveGames/profile folder
  SATISFACTORY_MODS_DIR          Override Satisfactory FactoryGame/Mods folder
  WEBKIT_DISABLE_DMABUF_RENDERER Set to 1 or 0 to override Linux WebKitGTK DMA-BUF behavior",
        env!("CARGO_PKG_VERSION")
    );
}

fn run_cli_mode(save_path_arg: Option<PathBuf>) -> Result<()> {
    let samples_dir = platform::resolve_samples_dir();
    let save_path = match save_path_arg {
        Some(p) => p,
        None => find_latest_save(&samples_dir).with_context(|| {
            format!(
                "Could not find any save file in {}",
                samples_dir.display()
            )
        })?,
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
    Ok(())
}

async fn bind_server_listener(
    requested_port: Option<u16>,
    allow_dynamic_fallback: bool,
) -> Result<tokio::net::TcpListener> {
    if let Some(port) = requested_port {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        return tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind server to {}", addr));
    }

    let default_addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    match tokio::net::TcpListener::bind(default_addr).await {
        Ok(listener) => Ok(listener),
        Err(e) if allow_dynamic_fallback => {
            tracing::debug!(
                "Port 8080 unavailable ({}), falling back to dynamic port",
                e
            );
            tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .context("Failed to bind server to dynamic port")
        }
        Err(e) => Err(e).with_context(|| format!("Failed to bind server to {}", default_addr)),
    }
}

fn run_http_server(requested_port: Option<u16>, open_browser: bool) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    rt.block_on(async move {
        let samples_dir = platform::resolve_samples_dir();
        let _ = fs::create_dir_all(&samples_dir);
        let state = AppState::new(&samples_dir);
        let app = create_app(state);

        let listener = bind_server_listener(requested_port, open_browser).await?;
        let local_addr = listener.local_addr().context("Failed to read bound address")?;
        let url = format!("http://{}", local_addr);

        println!("=======================================================");
        if open_browser {
            println!(" Satisfactory RWG Map Server (Browser Mode)");
        } else {
            println!(" Satisfactory RWG Map Server (Headless Mode)");
        }
        println!("=======================================================");
        println!("Serving on {}", url);

        if open_browser {
            if let Err(err) = platform::open_url_in_browser(&url) {
                tracing::warn!("Could not automatically open browser ({}): {}", url, err);
                eprintln!("Open {} manually in your web browser.", url);
            }
        }

        axum::serve(listener, app)
            .await
            .context("Server execution error")?;
        Ok(())
    })
}

#[cfg(feature = "desktop")]
fn unique_download_path(target: PathBuf) -> PathBuf {
    if !target.exists() {
        return target;
    }
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "download".to_string());
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1usize;
    loop {
        let candidate = parent.join(format!("{} ({}){}", stem, counter, ext));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(feature = "desktop")]
fn run_desktop_mode(requested_port: Option<u16>) -> Result<()> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    #[cfg(target_os = "linux")]
    {
        if !platform::has_linux_display() {
            bail!(
                "No X11 or Wayland display detected (DISPLAY and WAYLAND_DISPLAY are unset). \
                Run with --server for headless mode or --cli for command-line inspection."
            );
        }
        platform::configure_linux_webview_env();
    }

    let (tx_port, rx_port) = std::sync::mpsc::channel::<Result<u16, String>>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to initialize background runtime");
        rt.block_on(async move {
            let samples_dir = platform::resolve_samples_dir();
            let _ = fs::create_dir_all(&samples_dir);
            let state = AppState::new(&samples_dir);
            let app = create_app(state);

            match bind_server_listener(requested_port, true).await {
                Ok(listener) => {
                    let port = listener.local_addr().expect("local addr").port();
                    let _ = tx_port.send(Ok(port));
                    let _ = axum::serve(listener, app).await;
                }
                Err(err) => {
                    let _ = tx_port.send(Err(err.to_string()));
                }
            }
        });
    });

    let port = rx_port
        .recv()
        .context("Failed to receive local server port")?
        .map_err(anyhow::Error::msg)?;
    let url = format!("http://127.0.0.1:{}", port);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Satisfactory RWG Interactive Map")
        .with_inner_size(tao::dpi::LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(960.0, 600.0))
        .build(&event_loop)
        .context("Failed to create desktop application window")?;

    let webview_builder = WebViewBuilder::new()
        .with_url(&url)
        .with_download_started_handler(|_download_url, destination| {
            let downloads_dir = platform::resolve_downloads_dir();
            let _ = fs::create_dir_all(&downloads_dir);
            let file_name = destination
                .file_name()
                .map(|n| n.to_os_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| std::ffi::OsString::from("download.sav"));
            let target = downloads_dir.join(&file_name);
            if target != *destination {
                *destination = unique_download_path(target);
            }
            tracing::info!("Starting download to {:?}", destination);
            true
        })
        .with_download_completed_handler(|download_url, path, success| {
            if success {
                tracing::info!("Download completed ({}) -> {:?}", download_url, path);
            } else {
                tracing::warn!("Download failed ({}) -> {:?}", download_url, path);
            }
        });

    #[cfg(target_os = "linux")]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        let vbox = window
            .default_vbox()
            .context("Failed to retrieve GTK container box from window")?;
        webview_builder
            .build_gtk(vbox)
            .context("Failed to initialize WebKitGTK WebView")?
    };

    #[cfg(not(target_os = "linux"))]
    let _webview = webview_builder
        .build(&window)
        .context("Failed to initialize WebView")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let opts = parse_cli_options()?;

    if opts.help {
        print_help();
        return Ok(());
    }

    if opts.version {
        println!("sat-rwg-map {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if opts.cli {
        return run_cli_mode(opts.save_path);
    }

    if opts.server || opts.browser {
        return run_http_server(opts.port, opts.browser);
    }

    #[cfg(feature = "desktop")]
    {
        run_desktop_mode(opts.port)
    }

    #[cfg(not(feature = "desktop"))]
    {
        println!("Built without 'desktop' feature; starting headless HTTP server.");
        run_http_server(opts.port, opts.browser)
    }
}
