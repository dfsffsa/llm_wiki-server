#!/usr/bin/env bash
# Batch ingest all raw/sources/*.md for a project. Resumable via ingest cache.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${LLM_WIKI_PROJECT:?set LLM_WIKI_PROJECT}"
CONFIG="${LLM_WIKI_CONFIG:?set LLM_WIKI_CONFIG}"
LOG="${INGEST_LOG:-/tmp/llm-wiki-ingest-$(basename "$PROJECT").log}"

export LLM_WIKI_REPO="${LLM_WIKI_REPO:-$ROOT}"

# Relative --config is resolved against repo root (not caller cwd).
if [[ "$CONFIG" != /* ]]; then
  CONFIG="$ROOT/$CONFIG"
fi
if [[ ! -f "$CONFIG" ]]; then
  echo "error: config not found: $CONFIG" >&2
  exit 1
fi

if [[ ! -d "$PROJECT/raw/sources" ]]; then
  echo "error: missing $PROJECT/raw/sources" >&2
  exit 1
fi

shopt -s nullglob
sources=("$PROJECT/raw/sources"/*.md)
shopt -u nullglob
if [[ ${#sources[@]} -eq 0 ]]; then
  echo "error: no *.md in $PROJECT/raw/sources" >&2
  exit 1
fi

total=${#sources[@]}
n=0
ok=0
fail=0

echo "==> batch ingest: $PROJECT ($total files)" | tee -a "$LOG"
echo "==> config: $CONFIG" | tee -a "$LOG"
echo "==> log: $LOG" | tee -a "$LOG"

for f in "${sources[@]}"; do
  n=$((n + 1))
  base=$(basename "$f")
  if [[ -f "$PROJECT/wiki/sources/$base" ]]; then
    echo "==> [$n/$total] SKIP (already ingested): $base" | tee -a "$LOG"
    ok=$((ok + 1))
    continue
  fi
  echo "" | tee -a "$LOG"
  echo "==> [$n/$total] $base" | tee -a "$LOG"
  if "$ROOT/scripts/llm-wiki" ingest "$f" --project "$PROJECT" --config "$CONFIG" >>"$LOG" 2>&1; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    echo "FAILED: $base (see $LOG)" | tee -a "$LOG"
    exit 1
  fi
done

echo "" | tee -a "$LOG"
echo "==> done: ok=$ok fail=$fail total=$total" | tee -a "$LOG"
