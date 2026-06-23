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

# node_modules 必须本机已装（远端不跑 npm ci，1.6GB RAM 会 OOM）
if [[ ! -d "$CLI_NODE_MODULES_LOCAL" ]] || [[ ! -d "$UPSTREAM_NODE_MODULES_LOCAL" ]]; then
  echo "  缺少 node_modules（远端不再 npm ci，必须本机装好 rsync 过去）:" >&2
  [[ ! -d "$CLI_NODE_MODULES_LOCAL" ]] && echo "    - $CLI_NODE_MODULES_LOCAL" >&2
  [[ ! -d "$UPSTREAM_NODE_MODULES_LOCAL" ]] && echo "    - $UPSTREAM_NODE_MODULES_LOCAL" >&2
  echo "  请先在本机执行:" >&2
  echo "    npm ci --prefix ${ROOT}/overlay/cli/node" >&2
  echo "    npm ci --prefix ${ROOT}/upstream" >&2
  exit 1
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

# ─── 同步 upstream/src/ + 顶层配置（chat 子进程 @/ 别名解析需要）────
# cmd-llm-stream.ts 通过 @/lib/llm-client 等引用 upstream/src，Node 向上
# 查找 node_modules 时也需要 upstream/package.json + tsconfig.json。
# 不含 node_modules / dist（已单独同步），用 --exclude 排除避免重复。
echo "==> 同步 upstream/src/ + package.json + tsconfig（chat 子进程需要）"
rsync -avz --progress \
  --exclude='node_modules' \
  --exclude='dist' \
  --exclude='dist-ssr' \
  --exclude='.vite' \
  --include='*/' \
  --include='src/**' \
  --include='package.json' \
  --include='package-lock.json' \
  --include='tsconfig.json' \
  --include='tsconfig.node.json' \
  --exclude='*' \
  "${ROOT}/upstream/" \
  "${SSH_HOST}:${SERVER_REPO}/upstream/"

# ─── 同步 overlay/static/（landing + auth 页面） ─────────────
echo "==> 同步 overlay/static/（landing + auth HTML）"
rsync -avz --delete --progress \
  "${ROOT}/overlay/static"/ \
  "${SSH_HOST}:${SERVER_REPO}/overlay/static/"

# ─── 同步 overlay/cli/node/src/ + 配置（ingest/chat TS 脚本） ─
# cmd-ingest.ts / cmd-llm-stream.ts 在这里，rust 二进制通过 LLM_WIKI_REPO
# 找到它们。node_modules 已单独同步，这里只同步 src + package 元数据。
echo "==> 同步 overlay/cli/node/src/ + package.json + tsconfig"
rsync -avz --progress \
  --exclude='node_modules' \
  --include='*/' \
  --include='src/**' \
  --include='package.json' \
  --include='package-lock.json' \
  --include='tsconfig.json' \
  --exclude='*' \
  "${ROOT}/overlay/cli/node/" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/cli/node/"

# ─── 同步 node_modules（从本机 rsync，不在远端 npm ci） ─────
echo "==> 同步 overlay/cli/node/node_modules（约 35MB）"
rsync -avz --progress \
  --exclude='.cache' \
  "$CLI_NODE_MODULES_LOCAL"/ \
  "${SSH_HOST}:${SERVER_REPO}/overlay/cli/node/node_modules/"

echo "==> 同步 upstream/node_modules（约 524MB，首次较慢；增量 ~10s）"
rsync -avz --progress \
  --exclude='.cache' \
  --exclude='.vite' \
  "$UPSTREAM_NODE_MODULES_LOCAL"/ \
  "${SSH_HOST}:${SERVER_REPO}/upstream/node_modules/"

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
echo "  本脚本同步了全部运行时文件（二进制 + dist + src + node_modules + static）。"
echo "  远端不需要 git clone / git pull / npm ci。"
echo "  overlay/config/server.local.json 不在此同步（含密钥，手动管理）。"
echo "  全量首部署/换机器请用 deploy-ecs.sh。"
