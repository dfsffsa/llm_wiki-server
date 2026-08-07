#!/usr/bin/env bash
# 电子书批量入库编排(配置驱动)。
# 用法(子命令必须显式指定):
#   ./scripts/ebook_run.sh -c <batch.json> split|detect|check|fix|promote|pipeline [BOOK...]
# 配置模板: scripts/ebooks/batches/example.json
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -c|--config) CONFIG="$2"; shift 2 ;;
    -*) echo "unknown option: $1" >&2; exit 1 ;;
    *) break ;;
  esac
done
cmd="${1:-}"
shift || true
selected=("$@")

if [[ -z "$cmd" ]]; then
  echo "usage: $0 -c <batch.json> split|detect|check|fix|promote|pipeline [BOOK...]" >&2
  exit 2
fi

if [[ -z "$CONFIG" ]]; then
  echo "error: 需要 -c <batch.json>(模板见 scripts/ebooks/batches/example.json)" >&2
  exit 1
fi

# 配置路径解析:绝对直接用;相对依次试 scripts/ebooks/batches → .tools/ebooks/batches → cwd
if [[ "$CONFIG" != /* ]]; then
  for base in "$ROOT/scripts/ebooks/batches" "$ROOT/.tools/ebooks/batches" "$PWD"; do
    if [[ -f "$base/$CONFIG" ]]; then CONFIG="$base/$CONFIG"; break; fi
  done
fi
[[ -f "$CONFIG" ]] || { echo "error: config not found: $CONFIG" >&2; exit 1; }

# 读取配置(props: sourceDir outBase maxChars project)
IFS=$'\t' read -r SOURCE_DIR OUT_BASE MAX_CHARS PROJECT <<< "$(python3 "$ROOT/scripts/ebook_config.py" props "$CONFIG")"
PROJECT="${LLM_WIKI_PROJECT:-$PROJECT}"
if [[ -z "$PROJECT" ]]; then
  echo "error: 未指定 project(配置里或 LLM_WIKI_PROJECT env)" >&2
  exit 1
fi
PROJECT_DIR="$HOME/overseas-github/llm_wiki_projects/$PROJECT"

BOOK_ROWS="$(python3 "$ROOT/scripts/ebook_config.py" books "$CONFIG")"
[ -n "$BOOK_ROWS" ] || { echo "error: 配置里没有书" >&2; exit 1; }

is_selected() {
  [[ ${#selected[@]} -eq 0 ]] && return 0
  for s in "${selected[@]}"; do [[ "$s" == "$1" ]] && return 0; done
  return 1
}

do_split() {
  local book="$1" dir="$2" epub="$3" source="$4" hre="$5"
  local heading_args=()
  [[ -n "$hre" ]] && heading_args=(--heading-re "$hre")
  echo "==> split: $book"
  python3 "$ROOT/scripts/ebook_split.py" --epub "$SOURCE_DIR/$dir/$epub" \
    --book "$book" --source "$source" --out "$OUT_BASE/$book/chunks" \
    --max-chars "$MAX_CHARS" "${heading_args[@]}"
}

do_detect() {
  local book="$1" dir="$2" epub="$3"
  echo "==> detect: $book"
  python3 "$ROOT/scripts/ebook_detect.py" --epub "$SOURCE_DIR/$dir/$epub"
}

do_check() {
  local book="$1" fix="${2:-}"
  local fix_args=()
  [[ "$fix" == "--fix" ]] && fix_args=(--fix)
  echo "==> ${fix:+fix }check: $book"
  python3 "$ROOT/scripts/ebook_check.py" \
    --chunks "$OUT_BASE/$book/chunks" \
    --config "$ROOT/overlay/config/llm.judge.a.json" \
    --cache "$OUT_BASE/$book/check-cache.json" "${fix_args[@]}"
}

do_promote() {
  local book="$1"
  local chunks_dir="$OUT_BASE/$book/chunks"
  shopt -s nullglob
  local files=("$chunks_dir"/*.md)
  shopt -u nullglob
  if [[ ${#files[@]} -eq 0 ]]; then
    echo "    WARNING: no chunks for $book, skip promote" >&2
    return 0
  fi
  echo "==> promote: $book"
  mkdir -p "$PROJECT_DIR/raw/sources"
  cp "${files[@]}" "$PROJECT_DIR/raw/sources/"
  echo "    copied ${#files[@]} files to $PROJECT_DIR/raw/sources/"
}

while IFS=$'\t' read -r book dir epub source hre; do
  is_selected "$book" || continue
  case "$cmd" in
    split)   do_split "$book" "$dir" "$epub" "$source" "$hre" ;;
    detect)  do_detect "$book" "$dir" "$epub" ;;
    check)   do_check "$book" ;;
    fix)     do_check "$book" --fix ;;
    promote) do_promote "$book" ;;
    pipeline)
      do_split "$book" "$dir" "$epub" "$source" "$hre"
      do_check "$book"
      do_check "$book" --fix
      do_promote "$book"
      ;;
    *) echo "unknown cmd: $cmd (split|detect|check|fix|promote|pipeline)" >&2; exit 1 ;;
  esac
done <<< "$BOOK_ROWS"
