#!/usr/bin/env bash
# 增量同步 llm_wiki-server 的**构建产物**到远端。
#
# 与 deploy-ecs.sh 的区别：
#   - deploy-ecs.sh 是**全量部署**：覆盖 systemd unit、上传 server.local.json、
#     远端 npm ci、daemon-reload、restart。适合首次部署或大版本切换。
#   - sync-artifacts.sh 只同步二进制 / dist / node_modules。源码走 git，
#     systemd / config 不动。适合"git pull 完就想跑新代码"的常规迭代。
#
# 典型流程：
#   远端: git pull --recurse-submodules
#   本地: cargo build --release --target x86_64-unknown-linux-musl --manifest-path overlay/server/Cargo.toml
#   本地: cargo build --release --target x86_64-unknown-linux-musl --manifest-path overlay/cli/rust/Cargo.toml
#   本地: VITE_BACKEND=http VITE_API_TOKEN=... ./scripts/build-web.sh
#   本地: SSH_HOST=root@47.103.39.152 SSH_PORT=22022 ./scripts/sync-artifacts.sh
#   远端: systemctl restart llm-wiki-server    # 仅二进制 / dist 变化时
#
# 前提：
#   - 本机已用 musl 静态编译出
#       overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server
#       overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki
#   - 本机已 VITE_BACKEND=http 跑过 ./scripts/build-web.sh
#   - 远端已 git pull + 已 npm ci（首次 deploy 时用 deploy-ecs.sh）
set -euo pipefail

# ─── 必填参数 ────────────────────────────────────────────────────
: "${SSH_HOST:?SSH_HOST must be set, e.g. user@47.103.39.152 or an SSH alias}"

# ─── 可选参数（与 deploy-ecs.sh 保持一致） ─────────────────────
SSH_PORT="${SSH_PORT:-22}"
SSH_CONFIG="${SSH_CONFIG:-}"
SERVER_REPO="${SERVER_REPO:-/root/llm_wiki-server}"

# ─── 计算路径 ───────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SERVER_BIN_LOCAL="${ROOT}/overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server"
CLI_BIN_LOCAL="${ROOT}/overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki"
DIST_LOCAL="${ROOT}/upstream/dist"
CLI_NODE_MODULES_LOCAL="${ROOT}/overlay/cli/node/node_modules"
UPSTREAM_NODE_MODULES_LOCAL="${ROOT}/upstream/node_modules"
TOOLS_LOCAL="${ROOT}/.tools"

# ─── SSH / rsync 包装 ───────────────────────────────────────────
SSH_ARGS=()
RSYNC_RSH_ARGS=()
if [[ -n "${SSH_CONFIG}" ]]; then
  SSH_ARGS+=(-F "${SSH_CONFIG}")
  RSYNC_RSH_ARGS+=(-F "${SSH_CONFIG}")
fi
SSH_ARGS+=(-p "${SSH_PORT}")
RSYNC_RSH_ARGS+=(-p "${SSH_PORT}")
SSH=(ssh "${SSH_ARGS[@]}")
RSYNC_RSH="ssh ${RSYNC_RSH_ARGS[*]}"
export RSYNC_RSH

# ─── 打印参数摘要 ──────────────────────────────────────────────
echo "==> 同步参数"
echo "  SSH_HOST   = ${SSH_HOST}"
echo "  SSH_PORT   = ${SSH_PORT}"
echo "  SSH_CONFIG = ${SSH_CONFIG:-<none>}"
echo "  SERVER_REPO = ${SERVER_REPO}"

# ─── 检查前置（缺啥就明确报错） ──────────────────────────────
echo "==> 检查本机构建产物"
missing=()
for f in "$SERVER_BIN_LOCAL" "$CLI_BIN_LOCAL" "$DIST_LOCAL"; do
  if [[ ! -e "$f" ]]; then
    missing+=("$f")
  fi
done
if (( ${#missing[@]} > 0 )); then
  echo "  缺少构建产物（先 build 一下）:" >&2
  for f in "${missing[@]}"; do
    echo "    - $f" >&2
  done
  exit 1
fi
ls -lh "$SERVER_BIN_LOCAL" "$CLI_BIN_LOCAL"

# node_modules 是可选的（首次同步时可能还没装）
skip_node_modules=false
if [[ ! -d "$CLI_NODE_MODULES_LOCAL" ]] || [[ ! -d "$UPSTREAM_NODE_MODULES_LOCAL" ]]; then
  echo "  注意：本机缺 overlay/cli/node/node_modules 或 upstream/node_modules"
  echo "        远端如果是首次同步，请改用 deploy-ecs.sh（会跑 npm ci）"
  skip_node_modules=true
fi

# .tools/（protoc 等）也可选——只有远端要 cargo build 才会用
sync_tools=false
if [[ -d "$TOOLS_LOCAL" ]]; then
  sync_tools=true
fi

# ─── 测试 SSH ──────────────────────────────────────────────────
echo "==> 测试 SSH"
"${SSH[@]}" "$SSH_HOST" 'echo OK; uname -a'

# ─── 远端准备目标目录 ─────────────────────────────────────────
echo "==> 远端准备目标目录"
"${SSH[@]}" "$SSH_HOST" "mkdir -p \
  ${SERVER_REPO}/overlay/server/target/release \
  ${SERVER_REPO}/overlay/cli/rust/target/release \
  ${SERVER_REPO}/overlay/cli/node \
  ${SERVER_REPO}/upstream"

# ─── 同步 server 二进制 ──────────────────────────────────────
echo "==> 同步 server 二进制 (musl static)"
rsync -avz --progress \
  "$SERVER_BIN_LOCAL" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/server/target/release/llm-wiki-server"
"${SSH[@]}" "$SSH_HOST" "chmod +x ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server"

# ─── 同步 CLI 二进制 ─────────────────────────────────────────
echo "==> 同步 CLI 二进制 (musl static)"
rsync -avz --progress \
  "$CLI_BIN_LOCAL" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki"
"${SSH[@]}" "$SSH_HOST" "chmod +x ${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki"

# ─── 同步 UI dist（用 --delete：Vite 产物有 hash 旧 chunk 不会自动清） ─
echo "==> 同步 UI dist (--delete 清掉旧 chunk)"
rsync -avz --delete --progress \
  "$DIST_LOCAL"/ \
  "${SSH_HOST}:${SERVER_REPO}/upstream/dist/"

# ─── 同步 node_modules（可选；首次跳过） ─────────────────
if ! $skip_node_modules; then
  echo "==> 同步 overlay/cli/node/node_modules"
  rsync -avz --progress \
    --exclude='.cache' \
    "$CLI_NODE_MODULES_LOCAL"/ \
    "${SSH_HOST}:${SERVER_REPO}/overlay/cli/node/node_modules/"

  echo "==> 同步 upstream/node_modules"
  rsync -avz --progress \
    --exclude='.cache' \
    --exclude='.vite' \
    "$UPSTREAM_NODE_MODULES_LOCAL"/ \
    "${SSH_HOST}:${SERVER_REPO}/upstream/node_modules/"
fi

# ─── 同步 .tools/（protoc 等；远端 cargo build 才需要） ─────
if $sync_tools; then
  echo "==> 同步 .tools/（protoc 等）"
  rsync -avz --progress \
    "$TOOLS_LOCAL"/ \
    "${SSH_HOST}:${SERVER_REPO}/.tools/"
fi

# ─── 验证产物 ─────────────────────────────────────────────
echo "==> 远端核对"
"${SSH[@]}" "$SSH_HOST" "ls -lh \
  ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server \
  ${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki"
"${SSH[@]}" "$SSH_HOST" "file ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server"

echo "==> 完成"
echo "  远端重启服务（如改动了二进制或 dist）："
echo "    ssh ${SSH_ARGS[*]} ${SSH_HOST} 'systemctl restart llm-wiki-server'"
echo ""
echo "  源码（upstream/src、overlay/cli/node/src）走 git pull 同步，本脚本不动。"
echo "  package.json 改了需要远端手动 npm ci（不是常规改动）。"
echo "  全量首部署/换机器请用 deploy-ecs.sh。"
