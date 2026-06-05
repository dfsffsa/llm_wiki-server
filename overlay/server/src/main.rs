//! LLM Wiki headless HTTP server (overlay Phase 1).
//!
//! Serves upstream static UI + REST API compatible with upstream `api_server`.

mod api;
mod config;
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
}

fn main() {
    let args = Args::parse();
    let config = match ServerConfig::resolve(
        args.project,
        args.bind,
        args.config,
        args.static_dir,
        args.token,
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

    if let Err(err) = http_server::run(config) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
