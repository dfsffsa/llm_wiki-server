//! LLM Wiki headless HTTP server (overlay Phase 1).
//!
//! Serves upstream static UI + REST API compatible with upstream `api_server`.

mod api;
mod config;
mod llm;
mod server;
mod state;
mod static_files;

use std::sync::Arc;
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
    /// for CLI/e2e is unaffected.
    #[arg(long, env = "LLM_WIKI_REQUIRE_LOGIN", default_value_t = false)]
    require_login: bool,

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
}

fn main() {
    let args = Args::parse();
    let config = match ServerConfig::resolve(
        args.project,
        args.bind,
        args.config,
        args.static_dir,
        args.token,
        args.auth_db,
        args.require_login,
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

    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_flag = Arc::clone(&shutdown);
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("shutdown runtime");
        rt.block_on(async {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nShutting down...");
                shutdown_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                std::process::exit(0);
            }
        });
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
