mod cmd_ingest;
mod cmd_preprocess;
mod cmd_reindex;
mod cmd_rescan;
mod cmd_search;
mod cmd_vector;
mod config;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "llm-wiki",
    about = "LLM Wiki CLI — search, preprocess, rescan, reindex, ingest",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Keyword search over wiki pages
    Search {
        query: String,
        #[arg(long, env = "LLM_WIKI_PROJECT")]
        project: PathBuf,
        #[arg(long, default_value_t = 10)]
        top_k: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        include_content: bool,
    },
    /// Extract or copy source text (plain text formats; PDF/Office need desktop/pdfium)
    Preprocess {
        file: PathBuf,
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
        #[arg(long, help = "Copy file as-is when no text extraction is available")]
        copy_fallback: bool,
    },
    /// Scan raw/sources and emit file manifest (md5, size)
    Rescan {
        #[arg(long, env = "LLM_WIKI_PROJECT")]
        project: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Rebuild vector index (requires --vectors and config with embedding)
    Reindex {
        #[arg(long, env = "LLM_WIKI_PROJECT")]
        project: PathBuf,
        #[arg(long)]
        vectors: bool,
        #[arg(long, env = "LLM_WIKI_CONFIG")]
        config: Option<PathBuf>,
    },
    /// Run LLM ingest pipeline (Node/TS wrapper around upstream ingest.ts)
    Ingest {
        file: PathBuf,
        #[arg(long, env = "LLM_WIKI_PROJECT")]
        project: PathBuf,
        #[arg(long, env = "LLM_WIKI_CONFIG")]
        config: Option<PathBuf>,
    },
    /// Internal vector DB operations (used by Node ingest/reindex)
    #[command(hide = true)]
    Vector {
        #[command(subcommand)]
        command: VectorCommands,
    },
}

#[derive(Subcommand, Debug)]
enum VectorCommands {
    UpsertChunks {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        page_id: String,
    },
    DeletePage {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        page_id: String,
    },
    CountChunks {
        #[arg(long)]
        project: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Search {
            query,
            project,
            top_k,
            json,
            include_content,
        } => cmd_search::run(query, project, top_k, json, include_content),
        Commands::Preprocess {
            file,
            out,
            copy_fallback,
        } => cmd_preprocess::run(file, out, copy_fallback),
        Commands::Rescan { project, json } => cmd_rescan::run(project, json),
        Commands::Reindex {
            project,
            vectors,
            config,
        } => cmd_reindex::run(project, vectors, config).await,
        Commands::Ingest {
            file,
            project,
            config,
        } => cmd_ingest::run(file, project, config),
        Commands::Vector { command } => match command {
            VectorCommands::UpsertChunks { project, page_id } => {
                cmd_vector::upsert_chunks_from_stdin(project, page_id).await
            }
            VectorCommands::DeletePage { project, page_id } => {
                cmd_vector::delete_page(project, page_id).await
            }
            VectorCommands::CountChunks { project } => cmd_vector::count_chunks(project).await,
        },
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
