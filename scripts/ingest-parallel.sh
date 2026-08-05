#!/usr/bin/env bash
# 并行 ingest:把待入库源文件按序轮转分配到 N 个 worker,每个 worker 顺序 ingest。
# 与 ingest-batch.sh 一致:跳过 wiki/sources/<base> 已存在的文件。
# 用法:
#   INGEST_WORKERS=4 LLM_WIKI_PROJECT=... LLM_WIKI_CONFIG=... ./scripts/ingest-parallel.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${LLM_WIKI_PROJECT:?set LLM_WIKI_PROJECT}"
CONFIG="${LLM_WIKI_CONFIG:?set LLM_WIKI_CONFIG}"
WORKERS="${INGEST_WORKERS:-4}"
LIMIT="${INGEST_LIMIT:-0}"   # >0 时只处理前 N 个待入库文件(冒烟测试用)
export LLM_WIKI_REPO="${LLM_WIKI_REPO:-$ROOT}"

if [[ "$CONFIG" != /* ]]; then
  CONFIG="$ROOT/$CONFIG"
fi
if [[ ! -f "$CONFIG" ]]; then
  echo "error: config not found: $CONFIG" >&2
  exit 1
fi

shopt -s nullglob
all=("$PROJECT/raw/sources/"*.md)
shopt -u nullglob
if [[ ${#all[@]} -eq 0 ]]; then
  echo "error: no *.md in $PROJECT/raw/sources" >&2
  exit 1
fi

# 待入库 = 尚未在 wiki/sources/ 生成对应页的
pending=()
for f in "${all[@]}"; do
  base=$(basename "$f")
  [[ -f "$PROJECT/wiki/sources/$base" ]] && continue
  pending+=("$f")
done
if [[ "$LIMIT" -gt 0 ]] && [[ ${#pending[@]} -gt "$LIMIT" ]]; then
  pending=("${pending[@]:0:$LIMIT}")
fi
total=${#all[@]}
npend=${#pending[@]}
echo "==> parallel ingest: project=$PROJECT"
echo "==> total=$total pending=$npend workers=$WORKERS"
echo "==> config: $CONFIG"

if [[ $npend -eq 0 ]]; then
  echo "==> nothing to do"
  exit 0
fi

# 按序轮转分组:写入逐行文件列表(文件名可能含半角空格,如"CHAPTER 17",须用 read -r 逐行读)
TMPD="$(mktemp -d /tmp/ingest-groups.XXXXXX)"
trap 'rm -rf "$TMPD"' EXIT
for i in $(seq 0 $((WORKERS - 1))); do : > "$TMPD/group-$i.list"; done
i=0
for f in "${pending[@]}"; do
  printf '%s\n' "$f" >> "$TMPD/group-$i.list"
  i=$(( (i + 1) % WORKERS ))
done

pids=()
for i in $(seq 0 $((WORKERS - 1))); do
  list="$TMPD/group-$i.list"
  [[ -s "$list" ]] || continue
  (
    n=0
    while IFS= read -r f; do
      base=$(basename "$f")
      n=$((n + 1))
      if "$ROOT/scripts/llm-wiki" ingest "$f" --project "$PROJECT" --config "$CONFIG" >>"/tmp/ingest-w${i}.log" 2>&1; then
        echo "[w$i][$n] ok $base"
      else
        echo "[w$i][$n] FAILED $base"
      fi
    done < "$list"
    echo "[w$i] done ${n} files"
  ) &
  pids+=($!)
done

for p in "${pids[@]}"; do
  wait "$p"
done
echo "==> all workers done"
