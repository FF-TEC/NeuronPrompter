// =============================================================================
// NeuronPrompter binary entry point.
//
// Parses CLI arguments via clap to determine the execution mode (web, serve,
// mcp, version). When invoked without a subcommand, defaults to web UI mode:
// the axum server starts with the embedded SolidJS frontend and the default
// browser opens. In headless mode, the API server runs until interrupted.
// =============================================================================

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use neuronprompter_core::constants::{
    DEFAULT_PORT, DRAIN_TIMEOUT_SECS, LOG_CHANNEL_CAPACITY, MAX_SESSIONS, MODEL_CHANNEL_CAPACITY,
    PORT_SCAN_RANGE, RATE_LIMIT_REQUESTS, RATE_LIMIT_WINDOW_SECS, RATE_LIMITER_CLEANUP_SECS,
    SESSION_CLEANUP_SECS, SESSION_TTL_SECS,
};

/// NeuronPrompter: AI prompt management and organization tool.
///
/// When invoked without a subcommand, defaults to web UI mode (equivalent to
/// `neuronprompter web`). The embedded SolidJS frontend is served by axum and
/// the default browser opens at `http://localhost:3030`.
#[derive(Parser)]
#[command(name = "neuronprompter", version, about, long_about = None)]
struct Cli {
    /// Subcommand to execute. Defaults to `web` when omitted.
    #[command(subcommand)]
    command: Option<Command>,

    /// Logging verbosity level.
    #[arg(long, global = true, default_value = "info")]
    log_level: LogLevel,
}

/// CLI subcommands corresponding to the different execution modes.
#[derive(Subcommand)]
enum Command {
    /// Launch the web UI: starts the API server and opens the default browser.
    #[cfg(feature = "web")]
    Web {
        /// TCP port for the HTTP server.
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,

        /// Bind address for the HTTP server. Use "0.0.0.0" for LAN access.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Start the API server in headless mode without the browser-based frontend.
    Serve {
        /// TCP port for the HTTP server.
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,

        /// Bind address. Use "0.0.0.0" for remote access.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// MCP (Model Context Protocol) server for integration with AI assistants.
    #[cfg(feature = "mcp")]
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },

    /// Print the application version.
    Version,
}

/// Subcommands for the `mcp` command group.
#[cfg(feature = "mcp")]
#[derive(Subcommand)]
enum McpCommand {
    /// Start the MCP server in stdio mode.
    Serve,

    /// Register the NeuronPrompter MCP server in the specified client's config.
    Install {
        /// Target client: "claude-code" or "claude-desktop".
        #[arg(long, default_value = "claude-code")]
        target: String,
    },

    /// Remove the NeuronPrompter MCP server entry from the specified client's config.
    Uninstall {
        /// Target client: "claude-code" or "claude-desktop".
        #[arg(long, default_value = "claude-code")]
        target: String,
    },

    /// Check the current MCP registration status.
    Status {
        /// Target client. Omit to show all.
        #[arg(long)]
        target: Option<String>,
    },
}

/// Logging verbosity level.
#[derive(Clone, ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_filter_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

// ---------------------------------------------------------------------------
// Tracing initialization
// ---------------------------------------------------------------------------

fn init_tracing(level: &LogLevel) {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.as_filter_str()));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

/// Initializes tracing for the MCP stdio server (stderr only).
#[cfg(feature = "mcp")]
fn init_mcp_tracing(level: &LogLevel) {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.as_filter_str()));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

// ---------------------------------------------------------------------------
// Platform-specific paths -- delegated to neuronprompter_core::paths
// ---------------------------------------------------------------------------

fn db_path() -> Result<PathBuf, std::io::Error> {
    neuronprompter_core::paths::ensure_db_path()
}

// ---------------------------------------------------------------------------
// Server initialization
// ---------------------------------------------------------------------------

/// Initializes the database pool, runs migrations, resolves the active user,
/// and returns a fully-initialized `Arc<AppState>`.
fn init_app_state(is_localhost: bool) -> Result<Arc<neuronprompter_api::AppState>, String> {
    let db_path = db_path().map_err(|e| format!("failed to resolve db path: {e}"))?;

    let pool = neuronprompter_db::create_pool(&db_path)
        .map_err(|e| format!("failed to create database pool: {e}"))?;

    let (log_tx, _) = tokio::sync::broadcast::channel(LOG_CHANNEL_CAPACITY);
    let (model_tx, _) = tokio::sync::broadcast::channel(MODEL_CHANNEL_CAPACITY);

    // Generate a 256-bit cryptographic random shutdown token, matching the entropy
    // level of session tokens (32 bytes = 256 bits, hex-encoded to 64 characters).
    let session_token = {
        use rand::Rng as _;
        use std::fmt::Write as _;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        bytes.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    };

    let rate_limiter = Arc::new(
        neuronprompter_api::middleware::rate_limit::RateLimiter::new(
            RATE_LIMIT_REQUESTS,
            RATE_LIMIT_WINDOW_SECS,
        ),
    );

    Ok(Arc::new(neuronprompter_api::AppState::new(
        neuronprompter_api::state::AppStateConfig {
            pool,
            ollama: neuronprompter_api::state::OllamaState::new(),
            clipboard: neuronprompter_api::state::ClipboardState::new(),
            log_tx,
            model_tx,
            cancellation: tokio_util::sync::CancellationToken::new(),
            session_token,
            rate_limiter,
            sessions: neuronprompter_api::session::SessionStore::new(
                MAX_SESSIONS,
                std::time::Duration::from_secs(SESSION_TTL_SECS),
                is_localhost,
            ),
        },
    )))
}

/// Initializes app state, logging errors on failure.
fn try_init_app_state(is_localhost: bool) -> Option<Arc<neuronprompter_api::AppState>> {
    match init_app_state(is_localhost) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!("{e}");
            None
        }
    }
}

/// Spawns a background task that periodically cleans up expired rate limiter
/// entries, stopping when the cancellation token is triggered.
fn spawn_rate_limiter_cleanup(state: &Arc<neuronprompter_api::AppState>) {
    let limiter = state.rate_limiter.clone();
    let cancel = state.cancellation.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(RATE_LIMITER_CLEANUP_SECS));
        loop {
            tokio::select! {
                _ = interval.tick() => limiter.cleanup(),
                () = cancel.cancelled() => break,
            }
        }
    });
}

/// Spawns a background task that periodically cleans up expired sessions,
/// stopping when the cancellation token is triggered.
fn spawn_session_cleanup(state: &Arc<neuronprompter_api::AppState>) {
    let state_clone = state.clone();
    let cancel = state.cancellation.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(SESSION_CLEANUP_SECS));
        loop {
            tokio::select! {
                _ = interval.tick() => state_clone.sessions.cleanup_expired(),
                () = cancel.cancelled() => break,
            }
        }
    });
}

/// Returns `true` if the bind address is localhost.
fn is_localhost_bind(bind: &str) -> bool {
    matches!(bind.trim(), "127.0.0.1" | "::1" | "localhost")
}

// ---------------------------------------------------------------------------
// Entry point and command dispatch
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    // Default to web UI mode when no subcommand is provided.
    #[cfg(feature = "web")]
    let command = cli.command.unwrap_or(Command::Web {
        port: DEFAULT_PORT,
        bind: "127.0.0.1".to_owned(),
    });
    #[cfg(not(feature = "web"))]
    let command = cli.command.unwrap_or(Command::Serve {
        port: DEFAULT_PORT,
        bind: "127.0.0.1".to_owned(),
    });

    // MCP serve needs specialized tracing (stderr only).
    #[cfg(feature = "mcp")]
    let is_mcp_serve = matches!(
        &command,
        Command::Mcp {
            action: McpCommand::Serve
        }
    );
    #[cfg(not(feature = "mcp"))]
    let is_mcp_serve = false;

    // Web mode sets up its own layered tracing with BroadcastLayer.
    #[cfg(feature = "web")]
    let is_web = matches!(&command, Command::Web { .. });
    #[cfg(not(feature = "web"))]
    let is_web = false;

    if is_mcp_serve {
        #[cfg(feature = "mcp")]
        init_mcp_tracing(&cli.log_level);
    } else if !is_web {
        init_tracing(&cli.log_level);
    }

    let exit_code = match command {
        #[cfg(feature = "web")]
        Command::Web { port, bind } => run_web(port, bind, &cli.log_level),
        Command::Serve { port, bind } => run_serve(port, bind),
        #[cfg(feature = "mcp")]
        Command::Mcp { action } => run_mcp(action),
        Command::Version => {
            #[allow(clippy::print_stdout)]
            {
                println!("NeuronPrompter {}", env!("CARGO_PKG_VERSION"));
            }
            0
        }
    };

    std::process::exit(exit_code);
}

// ---------------------------------------------------------------------------
// Headless server mode
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)]
fn run_serve(port: u16, bind: String) -> i32 {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("failed to create tokio runtime: {e}");
            return 1;
        }
    };

    rt.block_on(async {
        tracing::info!(port = port, bind = %bind, "starting NeuronPrompter in headless mode");

        let Some(state) = try_init_app_state(is_localhost_bind(&bind)) else {
            return 1;
        };

        spawn_rate_limiter_cleanup(&state);
        spawn_session_cleanup(&state);

        let addr: std::net::SocketAddr = match format!("{bind}:{port}").parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("invalid bind address: {e}");
                return 1;
            }
        };

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("failed to bind to {addr}: {e}");
                return 1;
            }
        };

        tracing::info!(%addr, "HTTP server listening");

        let cancellation = state.cancellation.clone();
        let origin = format!("http://{bind}:{port}");
        let app = neuronprompter_api::build_router_headless(state, &origin);

        match axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            tokio::select! {
                () = cancellation.cancelled() => {},
                _ = tokio::signal::ctrl_c() => {},
            }
        })
        .await
        {
            Ok(()) => {
                tracing::info!("server shut down cleanly");
                0
            }
            Err(e) => {
                tracing::error!("server error: {e}");
                1
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Web UI mode
// ---------------------------------------------------------------------------

#[cfg(feature = "web")]
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
fn run_web(port: u16, bind: String, log_level: &LogLevel) -> i32 {
    // Create the log broadcast channel before initializing the tracing subscriber.
    let (log_tx, _) = tokio::sync::broadcast::channel::<String>(LOG_CHANNEL_CAPACITY);

    // Set up layered tracing: fmt + BroadcastLayer.
    {
        use tracing_subscriber::EnvFilter;
        use tracing_subscriber::layer::SubscriberExt;

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(log_level.as_filter_str()));

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false);

        let broadcast_layer =
            neuronprompter_web::broadcast_layer::BroadcastLayer::new(log_tx.clone());

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(broadcast_layer);

        if tracing::subscriber::set_global_default(subscriber).is_err() {
            tracing::error!("failed to set global tracing subscriber");
        }
    }

    // --- Decide GUI vs Browser path ---
    // On Linux, skip native GUI (wry/tao unreliable on Wayland) and use browser.
    #[cfg(all(feature = "gui", not(target_os = "linux")))]
    let preflight = neuronprompter_web::native_window::preflight_gui_check();
    #[cfg(all(feature = "gui", target_os = "linux"))]
    let preflight: Result<(), String> =
        Err("Linux: using browser for best compatibility".to_string());
    #[cfg(not(feature = "gui"))]
    let preflight: Result<(), String> = Err("gui feature disabled".to_string());

    if let Err(reason) = &preflight {
        tracing::info!(reason = %reason, "native GUI not available, using browser");
    }

    let use_gui = preflight.is_ok();

    if use_gui {
        // === GUI PATH ===
        // Main thread = tao event loop (platform requirement on macOS/Windows)
        // Background thread = tokio runtime + axum server
        // Bridge thread = waits for URL, forwards to event loop

        let (url_tx, url_rx) = std::sync::mpsc::channel::<String>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn server in a background thread with its own tokio runtime.
        let server_thread = std::thread::Builder::new()
            .name("axum-server".into())
            .spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("failed to create tokio runtime: {e}");
                        return 1;
                    }
                };

                rt.block_on(async {
                    tracing::info!(
                        port = port,
                        bind = %bind,
                        "starting NeuronPrompter in web UI mode (native window)"
                    );

                    let Some(state) = try_init_app_state(is_localhost_bind(&bind)) else {
                        return 1;
                    };

                    spawn_rate_limiter_cleanup(&state);
                    spawn_session_cleanup(&state);

                    // Bind TCP listener with port fallback.
                    let Some((listener, actual_port)) = bind_listener(&bind, port).await else {
                        return 1;
                    };

                    let url = format!("http://{bind}:{actual_port}");
                    tracing::info!(url = %url, "NeuronPrompter web server listening");

                    let web_state =
                        Arc::new(neuronprompter_web::WebState::new(state.clone(), log_tx));
                    let origin = format!("http://{bind}:{actual_port}");
                    let app =
                        neuronprompter_web::build_web_router(state.clone(), web_state, &origin);

                    // Send URL to GUI thread BEFORE starting to serve.
                    let _ = url_tx.send(url);

                    // Drain channel: separates "stop accepting" from "wait for completion".
                    // The drain timeout prevents orphaned SSE streams (log, model events)
                    // from blocking process exit indefinitely after the WebView is destroyed.
                    let (drain_tx, drain_rx) = tokio::sync::oneshot::channel::<()>();
                    let cancellation = state.cancellation.clone();

                    let serve_task = tokio::spawn(async move {
                        axum::serve(
                            listener,
                            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                        )
                        .with_graceful_shutdown(async move {
                            let _ = drain_rx.await;
                        })
                        .await
                    });

                    // Wait for shutdown signal from GUI (window close) or Ctrl+C.
                    tokio::select! {
                        _ = shutdown_rx => {
                            tracing::info!("window closed, starting graceful drain");
                        },
                        () = cancellation.cancelled() => {
                            tracing::info!("cancellation requested, starting graceful drain");
                        },
                        _ = tokio::signal::ctrl_c() => {
                            tracing::info!("Ctrl+C received, starting graceful drain");
                        },
                    }

                    // Signal axum to stop accepting new connections.
                    let _ = drain_tx.send(());

                    // Wait up to 5 seconds for existing connections to close.
                    // Long-lived SSE connections may never close on their own
                    // because the WebView that was reading them is already gone.
                    let drain_timeout_secs: u64 = DRAIN_TIMEOUT_SECS;
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(drain_timeout_secs),
                        serve_task,
                    )
                    .await
                    {
                        Ok(Ok(Ok(()))) => {
                            tracing::info!("web server shut down cleanly");
                            0
                        }
                        Ok(Ok(Err(e))) => {
                            tracing::error!("server error: {e}");
                            1
                        }
                        Ok(Err(join_err)) => {
                            tracing::error!("server task join error: {join_err}");
                            1
                        }
                        Err(_elapsed) => {
                            tracing::warn!(
                                timeout_secs = drain_timeout_secs,
                                "graceful drain timed out; forcing exit"
                            );
                            0
                        }
                    }
                })
            });

        let server_thread = match server_thread {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("failed to spawn server thread: {e}");
                return 1;
            }
        };

        // Main thread: run native GUI (blocks until window closed).
        #[cfg(feature = "gui")]
        if let Err(e) = neuronprompter_web::native_window::run_gui_with_splash(url_rx, shutdown_tx)
        {
            tracing::error!("GUI error: {e}");
        }

        server_thread.join().unwrap_or_else(|panic_val| {
            tracing::error!("server thread panicked: {panic_val:?}");
            1
        })
    } else {
        // === BROWSER PATH ===
        // Main thread = tokio runtime (no tao needed)

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("failed to create tokio runtime: {e}");
                return 1;
            }
        };

        rt.block_on(async {
            tracing::info!(
                port = port,
                bind = %bind,
                "starting NeuronPrompter in web UI mode (browser)"
            );

            let Some(state) = try_init_app_state(is_localhost_bind(&bind)) else {
                return 1;
            };

            spawn_rate_limiter_cleanup(&state);
            spawn_session_cleanup(&state);

            let Some((listener, actual_port)) = bind_listener(&bind, port).await else {
                return 1;
            };

            let url = format!("http://{bind}:{actual_port}");
            tracing::info!(url = %url, "NeuronPrompter web server listening");

            let web_state = Arc::new(neuronprompter_web::WebState::new(state.clone(), log_tx));
            let origin = format!("http://{bind}:{actual_port}");
            let app = neuronprompter_web::build_web_router(state.clone(), web_state, &origin);

            neuronprompter_web::browser::launch_browser(&url);

            let cancellation = state.cancellation.clone();
            match axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                tokio::select! {
                    () = cancellation.cancelled() => {},
                    _ = tokio::signal::ctrl_c() => {},
                }
            })
            .await
            {
                Ok(()) => {
                    tracing::info!("web server shut down cleanly");
                    0
                }
                Err(e) => {
                    tracing::error!("server error: {e}");
                    1
                }
            }
        })
    }
}

/// Binds a TCP listener with port fallback (tries port..port+19).
#[cfg(feature = "web")]
async fn bind_listener(bind: &str, port: u16) -> Option<(tokio::net::TcpListener, u16)> {
    let mut last_err = None;
    for candidate in port..=port.saturating_add(PORT_SCAN_RANGE - 1) {
        let addr_str = format!("{bind}:{candidate}");
        let addr: std::net::SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("invalid bind address: {e}");
                return None;
            }
        };
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                if candidate != port {
                    tracing::info!(
                        requested_port = port,
                        actual_port = candidate,
                        "preferred port occupied, using fallback"
                    );
                }
                return Some((l, candidate));
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }
    tracing::error!(
        "failed to bind to ports {port}--{}; last error: {}",
        port.saturating_add(PORT_SCAN_RANGE - 1),
        last_err.map(|e| e.to_string()).unwrap_or_default()
    );
    None
}

// ---------------------------------------------------------------------------
// MCP mode
// ---------------------------------------------------------------------------

#[cfg(feature = "mcp")]
#[allow(clippy::too_many_lines)]
fn run_mcp(action: McpCommand) -> i32 {
    match action {
        McpCommand::Serve => {
            let db_path = match db_path() {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("failed to resolve db path: {e}");
                    return 1;
                }
            };

            // Open a single-connection Database for the MCP server.
            // MCP runs as a child process with its own DB handle, sharing
            // the same SQLite file via WAL mode for concurrent reads.
            let db = match neuronprompter_db::Database::open(&db_path) {
                Ok(db) => std::sync::Arc::new(db),
                Err(e) => {
                    tracing::error!("failed to open database: {e}");
                    return 1;
                }
            };

            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("failed to create tokio runtime: {e}");
                    return 1;
                }
            };

            rt.block_on(async {
                if let Err(e) = neuronprompter_mcp::server::run_stdio_server(db).await {
                    tracing::error!("MCP server error: {e}");
                    return 1;
                }
                0
            })
        }
        McpCommand::Install { target } => {
            let mcp_target = match neuronprompter_mcp::registration::parse_target(&target) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("{e}");
                    return 1;
                }
            };
            match neuronprompter_mcp::registration::install(None, mcp_target) {
                Ok(msg) => {
                    tracing::info!("{msg}");
                    0
                }
                Err(e) => {
                    tracing::error!("MCP install failed: {e}");
                    1
                }
            }
        }
        McpCommand::Uninstall { target } => {
            let mcp_target = match neuronprompter_mcp::registration::parse_target(&target) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("{e}");
                    return 1;
                }
            };
            match neuronprompter_mcp::registration::uninstall(mcp_target) {
                Ok(msg) => {
                    tracing::info!("{msg}");
                    0
                }
                Err(e) => {
                    tracing::error!("MCP uninstall failed: {e}");
                    1
                }
            }
        }
        McpCommand::Status { target } => {
            let targets: Vec<neuronprompter_mcp::registration::McpTarget> = match target {
                Some(ref t) => match neuronprompter_mcp::registration::parse_target(t) {
                    Ok(parsed) => vec![parsed],
                    Err(e) => {
                        tracing::error!("{e}");
                        return 1;
                    }
                },
                None => neuronprompter_mcp::registration::McpTarget::all().to_vec(),
            };
            for t in targets {
                let status = neuronprompter_mcp::registration::check_status(t);
                let config = status.config_path.as_deref().unwrap_or("unknown");
                if status.registered {
                    tracing::info!(
                        target = t.cli_name(),
                        config_path = config,
                        exe = status.exe_path.as_deref().unwrap_or("unknown"),
                        "registered"
                    );
                } else {
                    tracing::info!(
                        target = t.cli_name(),
                        config_path = config,
                        "not registered"
                    );
                }
            }
            0
        }
    }
}
