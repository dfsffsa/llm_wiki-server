//! LLM Wiki headless HTTP server (overlay Phase 1).
//!
//! Serves upstream static UI + REST API compatible with upstream `api_server`.

mod api;
mod config;
mod llm;
mod mail;
mod metrics;
mod server;
mod state;
mod static_files;

use std::thread;

use clap::Parser;

use crate::config::ServerConfig;
use crate::server as http_server;

#[derive(Parser, Debug)]
#[command(name = "llm-wiki-server", about = "Headless LLM Wiki HTTP server (overlay)")]
struct Args {
    /// Wiki project root directory
    #[arg(long, env = "LLM_WIKI_PROJECT")]
    project: Option<String>,

    /// API bearer token (overrides config file token)
    #[arg(long, env = "LLM_WIKI_API_TOKEN")]
    token: Option<String>,

    /// Listen address (host:port)
    #[arg(long, env = "LLM_WIKI_BIND", default_value = "127.0.0.1:8080")]
    bind: String,

    /// Server config JSON (embedding, api, etc.)
    #[arg(long, env = "LLM_WIKI_CONFIG")]
    config: Option<String>,

    /// Static UI directory (default: upstream/dist if present)
    #[arg(long, env = "LLM_WIKI_STATIC")]
    static_dir: Option<String>,

    /// SQLite path for auth/history/usage. If unset, multi-user mode is off.
    #[arg(long, env = "LLM_WIKI_AUTH_DB")]
    auth_db: Option<String>,

    /// Require login on the lite page (browser users). Bearer token auth
    /// for CLI/e2e is unaffected. When true, anonymous (no-cookie, no-bearer)
    /// API access is rejected even if apiConfig.allowUnauthenticated is true
    /// in the config file — a deploy-time hard switch that can't be relaxed
    /// by config.
    #[arg(long, env = "LLM_WIKI_REQUIRE_LOGIN", default_value_t = false)]
    require_login: bool,

    /// Close public registration. When true, POST /auth/register returns 403.
    /// Use for invite-only / operator-provisioned accounts.
    #[arg(long, env = "LLM_WIKI_DISABLE_REGISTRATION", default_value_t = false)]
    disable_registration: bool,

    /// Per-user daily chat limit (cookie-authenticated requests only).
    #[arg(long, env = "LLM_WIKI_DAILY_CHAT_LIMIT", default_value_t = 50)]
    daily_chat_limit: u32,

    /// Email that is auto-marked admin on registration.
    #[arg(long, env = "LLM_WIKI_ADMIN_EMAIL")]
    admin_email: Option<String>,

    /// Session cookie lifetime in days.
    #[arg(long, env = "LLM_WIKI_SESSION_TTL_DAYS", default_value_t = 30)]
    session_ttl_days: u32,

    /// Directory containing the public landing page (index.html etc.). When
    /// set, requests to `/` and `/login`/`/register`/`/reset-password` are
    /// served from here instead of upstream/dist.
    #[arg(long, env = "LLM_WIKI_PUBLIC_LANDING_DIR")]
    public_landing_dir: Option<String>,

    /// Grace period (seconds) to drain in-flight requests on SIGINT/SIGTERM
    /// before forcing exit. Long-lived SSE chats still holding a slot past
    /// this window are cut off. Should be ≤ systemd `TimeoutStopSec`.
    #[arg(long, env = "LLM_WIKI_DRAIN_SECS", default_value_t = 15)]
    drain_secs: u64,
}

fn main() {
    // Structured logging: RUST_LOG controls verbosity (default info).
    // Format: timestamp level [request_id] message — the request_id comes
    // from the per-request tracing span set in dispatch_request.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let args = Args::parse();
    let config = match ServerConfig::resolve(
        args.project,
        args.bind,
        args.config,
        args.static_dir,
        args.token,
        args.auth_db,
        args.require_login,
        args.disable_registration,
        args.daily_chat_limit,
        args.admin_email,
        args.session_ttl_days,
        args.public_landing_dir,
    ) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };

    // Graceful shutdown: on SIGINT or SIGTERM, stop accepting new requests
    // and give in-flight work up to `drain_secs` to finish before forcing
    // exit. tiny_http's blocking accept can't be interrupted, so the accept
    // loop fast-rejects new connections (503) once `request_shutdown()` is
    // called; this thread waits for the in-flight count to drain, then exits.
    let drain_secs = args.drain_secs;
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("shutdown runtime");
        rt.block_on(async {
            let ctrl_c = tokio::signal::ctrl_c();
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
            tokio::select! {
                _ = ctrl_c => {}
                _ = term.recv() => {}
            }
        });
        eprintln!("\n[shutdown] signal received; draining in-flight requests (up to {drain_secs}s)...");
        crate::api::request_shutdown();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(drain_secs);
        loop {
            let in_flight = crate::api::in_flight_count();
            if in_flight == 0 {
                eprintln!("[shutdown] all in-flight requests drained");
                break;
            }
            if std::time::Instant::now() >= deadline {
                eprintln!("[shutdown] drain timeout reached; {in_flight} request(s) still in-flight, forcing exit");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        std::process::exit(0);
    });

    // Shared multi-thread runtime for embedding + chat streaming (reqwest).
    // Worker threads block_on onto it; chat streams are bounded by
    // MAX_CONCURRENT_CHAT so they can't exhaust the pool.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => Some(std::sync::Arc::new(rt)),
        Err(e) => {
            eprintln!("error: failed to build async runtime: {e}");
            std::process::exit(1);
        }
    };

    let auth_service = match &config.auth_db {
        Some(path) => {
            let store = match llm_wiki_auth::Store::open(path) {
                Ok(s) => std::sync::Arc::new(s),
                Err(e) => {
                    eprintln!("auth: failed to open SQLite at {}: {e}", path.display());
                    std::process::exit(1);
                }
            };
            let svc = llm_wiki_auth::AuthService::new(store, llm_wiki_auth::AuthServiceConfig {
                session_ttl_secs: (config.session_ttl_days as i64) * 24 * 3600,
                admin_email: config.admin_email.clone(),
                login_attempts: 25.0,
                login_period_secs: 3600.0,
            });
            Some(std::sync::Arc::new(svc))
        }
        None => None,
    };

    if let Err(err) = http_server::run(config, auth_service, runtime) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
