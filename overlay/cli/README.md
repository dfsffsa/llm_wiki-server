# LLM Wiki CLI (Phase 3)

Unified entry: `./scripts/llm-wiki` (wrapper around Rust binary + Node helpers).

## Build

```bash
./scripts/build-cli.sh
```

Installs `protoc` under `.tools/` when needed (LanceDB dependency). Requires Node for `ingest` / `reindex --vectors`.

## Commands

| Command | Layer | Description |
|---------|-------|-------------|
| `llm-wiki search "query" --project PATH` | Rust | Keyword search over `wiki/` |
| `llm-wiki preprocess FILE [-o OUT] [--copy-fallback]` | Rust | Plain-text extract; PDF/Office needs desktop or `--copy-fallback` |
| `llm-wiki rescan --project PATH [--json]` | Rust | Scan `raw/sources` manifest (md5, size) |
| `llm-wiki reindex --project PATH [--vectors] [--config PATH]` | Rust + Node | Count wiki pages; `--vectors` rebuilds LanceDB via upstream `embedding.ts` |
| `llm-wiki ingest FILE --project PATH --config PATH` | Node | Wraps upstream `ingest.ts` (needs LLM API key in config) |

Hidden vector subcommands (`vector upsert-chunks`, etc.) are used by the Node shim for LanceDB; not intended for direct use.

## Examples

```bash
export LLM_WIKI_PROJECT=/data/my-wiki

./scripts/llm-wiki search "transformer" --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki rescan --project "$LLM_WIKI_PROJECT" --json
./scripts/llm-wiki preprocess note.md -o raw/sources/note.txt

# Vector reindex + ingest need config (see overlay/config/llm.example.json)
export LLM_WIKI_CONFIG=overlay/config/llm.json
./scripts/llm-wiki reindex --vectors --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki ingest doc.pdf --project "$LLM_WIKI_PROJECT"
```

## Layout

- `rust/` — clap CLI (`search`, `preprocess`, `rescan`, `reindex` orchestration, LanceDB vector ops)
- `node/` — `cmd-ingest.ts`, `cmd-reindex.ts` with Tauri/fs shims calling upstream TS
- `../crates/llm-wiki-common/` — shared search + rescan (also used by headless server)

## Environment

| Variable | Purpose |
|----------|---------|
| `LLM_WIKI_PROJECT` | Wiki project root (contains `wiki/`, `raw/`) |
| `LLM_WIKI_CONFIG` | JSON with `llmConfig`, optional `embeddingConfig` |
| `LLM_WIKI_REPO` | Set automatically by `./scripts/llm-wiki` |
| `LLM_WIKI_BIN` | Path to Rust binary (set by Rust when spawning Node) |
