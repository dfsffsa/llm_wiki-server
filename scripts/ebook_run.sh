#!/usr/bin/env bash
# 电子书批量入库编排:书单 + split/check/promote 子命令。
# 用法:
#   ./scripts/ebook_run.sh split [BOOK...]     # 转换+切分到 .tools staging
#   ./scripts/ebook_run.sh check [BOOK...]     # LLM 语义检查(逐本,断点续跑)
#   ./scripts/ebook_run.sh promote [BOOK...]   # 拷 chunks 到 raw/sources
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_BASE="/mnt/c/Users/Lenovo/Downloads/电子书"
OUT_BASE="$ROOT/.tools/ebooks"
PROJECT="${LLM_WIKI_PROJECT:-$HOME/overseas-github/llm_wiki_projects/ParentingBooks}"

# 简化书名|epub相对路径|原书名(frontmatter source)
BOOKS=(
  "法伯睡眠宝典|1454-法伯睡眠宝典/法伯睡眠宝典.epub|法伯睡眠宝典"
  "崔玉涛自然养育法|11063-崔玉涛自然养育法/CuiYuTaoZiRanYangYuFa(J.epub|崔玉涛自然养育法"
  "好孕从卵子开始|11153-好孕，从卵子开始/HaoYunCongLuanZiKaiShi.epub|好孕，从卵子开始"
  "成就好爸爸|2548-成就好爸爸：男人一生最重要的工作/成就好爸爸：男人一生最重要的工作.epub|成就好爸爸：男人一生最重要的工作"
  "定本育儿百科|290-定本育儿百科/DingBenYuErBaiKe.epub|定本育儿百科"
  "西尔斯育儿经|291-西尔斯育儿经/XiErSiYuErJing.epub|西尔斯育儿经"
  "第一次当奶爸|738-第一次当奶爸/第一次当奶爸.epub|第一次当奶爸"
  "养育女孩|9028-养育女孩（成长版）/YangYuNuHai.epub|养育女孩（成长版）"
)

# 逐书 heading-re 覆盖(正则不匹配默认 "第N章　" 全角空格时指定)
# 注意:全角空格(U+3000) / 全角句点(U+FF0E) 直接以 UTF-8 字节写入即可
declare -A HEADING_RE=(
  ["崔玉涛自然养育法"]='^[0-9]{2} '
  ["成就好爸爸"]='^[0-9]+．'
  ["定本育儿百科"]='^\d+\.'
  ["西尔斯育儿经"]='^(CHAPTER [0-9]+|Part [IVX]+)　'
  ["第一次当奶爸"]='^第[一二三四五六七八九十百千]+章 '
  ["养育女孩"]='^第[一二三四五六七八九十百千]+章$'
)

cmd="${1:-split}"
shift || true
selected=("$@")

for entry in "${BOOKS[@]}"; do
  IFS='|' read -r book rel source <<< "$entry"
  if [[ ${#selected[@]} -gt 0 ]]; then
    keep=0
    for s in "${selected[@]}"; do [[ "$s" == "$book" ]] && keep=1; done
    [[ $keep -eq 0 ]] && continue
  fi
  case "$cmd" in
    split)
      echo "==> split: $book"
      heading_args=()
      if [[ -n "${HEADING_RE[$book]:-}" ]]; then
        heading_args=(--heading-re "${HEADING_RE[$book]}")
      fi
      python3 "$ROOT/scripts/ebook_split.py" --epub "$SRC_BASE/$rel" \
        --book "$book" --source "$source" --out "$OUT_BASE/$book/chunks" \
        "${heading_args[@]}"
      ;;
    check)
      echo "==> check: $book"
      python3 "$ROOT/scripts/ebook_check.py" \
        --chunks "$OUT_BASE/$book/chunks" \
        --config "$ROOT/overlay/config/llm.judge.a.json" \
        --cache "$OUT_BASE/$book/check-cache.json" \
        --report "$OUT_BASE/$book/report.md"
      ;;
    promote)
      echo "==> promote: $book"
      mkdir -p "$PROJECT/raw/sources"
      cp "$OUT_BASE/$book/chunks/"*.md "$PROJECT/raw/sources/"
      echo "    copied to $PROJECT/raw/sources/"
      ;;
    *) echo "unknown cmd: $cmd (split|check|promote)" >&2; exit 1 ;;
  esac
done
