//! LLM Wiki headless server — Phase 1 placeholder.
//!
//! Planned: static UI from `upstream/dist` + HTTP API (extend upstream api_server).

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "llm-wiki-server", about = "Headless LLM Wiki HTTP server (overlay)")]
struct Args {
    /// Wiki project root directory
    #[arg(long, env = "LLM_WIKI_PROJECT")]
    project: Option<String>,

    /// API bearer token
    #[arg(long, env = "LLM_WIKI_API_TOKEN")]
    token: Option<String>,

    /// Listen address (host:port)
    #[arg(long, env = "LLM_WIKI_BIND", default_value = "127.0.0.1:8080")]
    bind: String,

    /// Server config JSON (embedding, api, etc.)
    #[arg(long, env = "LLM_WIKI_CONFIG")]
    config: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    eprintln!("llm-wiki-server: Phase 1 skeleton");
    eprintln!("  bind:    {}", args.bind);
    eprintln!("  project: {:?}", args.project.as_deref().unwrap_or("(not set)"));
    eprintln!("  config:  {:?}", args.config.as_deref().unwrap_or("(not set)"));
    eprintln!();
    eprintln!("See docs/ARCHITECTURE.md for implementation plan.");
    std::process::exit(0);
}
