#!/usr/bin/env bash
# 一键部署 llm_wiki-server 到低配 ECS。
#
# 适用场景：服务器无法本地编译（1.6GB RAM 等），musl 静态二进制 + Vite
# 预构建产物 + Node 依赖一起 rsync 上传。
#
# 前提：
#   - 本机已用 musl 静态编译出
#       overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server
#       overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki
#   - 本机已 npm run build 出 upstream/dist/
#   - SSH 端口可达（默认 22；被封时改 22022）
#
# 用法：
#   SSH_HOST=user@47.103.39.152 LLM_API_KEY='sk-...' ./scripts/deploy-ecs.sh
#
#   # 自定义 SSH 端口（阿里云安全组只开 22022 时）
#   SSH_HOST=user@47.103.39.152 SSH_PORT=22022 LLM_API_KEY='sk-...' \
#     ./scripts/deploy-ecs.sh
#
#   # SSH config 含 Host 别名时
#   SSH_HOST=llm-wiki-ecs SSH_CONFIG=~/.ssh/config.d/ecs.conf \
#     LLM_API_KEY='sk-...' ./scripts/deploy-ecs.sh
#
#   # 全部参数走 env
#   SSH_HOST=... SSH_PORT=... SSH_CONFIG=... \
#   SERVER_REPO=/root/llm_wiki-server \
#   SERVER_WIKI_ROOT=/root/llm_wiki_projects \
#   SERVER_PORT=8081 \
#   LLM_API_KEY='sk-...' \
#     ./scripts/deploy-ecs.sh
set -euo pipefail

# ─── 必填参数 ────────────────────────────────────────────────────
: "${SSH_HOST:?SSH_HOST must be set, e.g. user@47.103.39.152 or an SSH alias}"
: "${LLM_API_KEY:?LLM_API_KEY must be set (no default; never hardcode)}"

# ─── 可选参数（带默认值） ───────────────────────────────────────
SSH_PORT="${SSH_PORT:-22}"
SSH_CONFIG="${SSH_CONFIG:-}"                       # 留空 = 不带 -F
SERVER_REPO="${SERVER_REPO:-/root/llm_wiki-server}"
SERVER_WIKI_ROOT="${SERVER_WIKI_ROOT:-/root/llm_wiki_projects}"
SERVER_PORT="${SERVER_PORT:-8080}"
SERVER_BIND="${SERVER_BIND:-127.0.0.1:${SERVER_PORT}}"
SERVER_TOKEN="${SERVER_TOKEN:-minmax2.7}"          # 缺省值；生产建议改

# ─── 计算路径 ───────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SERVER_BIN_LOCAL="${ROOT}/overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server"
CLI_BIN_LOCAL="${ROOT}/overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki"
DIST_LOCAL="${ROOT}/upstream/dist"
CONFIG_LOCAL="${ROOT}/overlay/config/server.example.json"

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

# ─── 打印参数摘要（密钥脱敏） ──────────────────────────────────
echo "==> 部署参数"
echo "  SSH_HOST         = ${SSH_HOST}"
echo "  SSH_PORT         = ${SSH_PORT}"
echo "  SSH_CONFIG       = ${SSH_CONFIG:-<none>}"
echo "  SERVER_REPO      = ${SERVER_REPO}"
echo "  SERVER_WIKI_ROOT = ${SERVER_WIKI_ROOT}"
echo "  SERVER_BIND      = ${SERVER_BIND}"
echo "  SERVER_TOKEN     = ${SERVER_TOKEN}"
echo "  LLM_API_KEY      = ${LLM_API_KEY:0:8}…${LLM_API_KEY: -4}  ($(printf '%s' "$LLM_API_KEY" | wc -c) chars)"

# ─── 检查前置 ──────────────────────────────────────────────────
echo "==> 检查本机构建产物"
for f in "$SERVER_BIN_LOCAL" "$CLI_BIN_LOCAL" "$DIST_LOCAL" "$CONFIG_LOCAL"; do
  if [[ ! -e "$f" ]]; then
    echo "  缺少: $f" >&2
    exit 1
  fi
done
ls -lh "$SERVER_BIN_LOCAL" "$CLI_BIN_LOCAL"
file "$SERVER_BIN_LOCAL" | head -1
file "$CLI_BIN_LOCAL" | head -1

# ─── 测试 SSH ──────────────────────────────────────────────────
echo "==> 测试 SSH"
"${SSH[@]}" "$SSH_HOST" 'echo OK; uname -a; df -h /root | tail -1'

# ─── 准备远端目录 ─────────────────────────────────────────────
echo "==> 准备远端目录"
"${SSH[@]}" "$SSH_HOST" "mkdir -p \
  ${SERVER_REPO}/overlay/server/target/release \
  ${SERVER_REPO}/overlay/cli/rust/target/release \
  ${SERVER_REPO}/overlay/cli/node \
  ${SERVER_REPO}/overlay/config \
  ${SERVER_REPO}/upstream"

# ─── 上传 server 二进制 ───────────────────────────────────────
echo "==> 上传 server 二进制"
rsync -avz --progress \
  "$SERVER_BIN_LOCAL" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/server/target/release/llm-wiki-server"
"${SSH[@]}" "$SSH_HOST" "chmod +x ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server"

# ─── 上传 CLI 二进制 ──────────────────────────────────────────
echo "==> 上传 CLI 二进制"
rsync -avz --progress \
  "$CLI_BIN_LOCAL" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki"
"${SSH[@]}" "$SSH_HOST" "chmod +x ${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki"

# ─── 上传 UI ─────────────────────────────────────────────────
echo "==> 上传 UI (upstream/dist/)"
rsync -avz --delete --progress \
  "$DIST_LOCAL"/ \
  "${SSH_HOST}:${SERVER_REPO}/upstream/dist/"

# ─── 上传 upstream/src/（chat 子进程通过 @/ 别名解析到这里）────
echo "==> 上传 upstream/src/（chat 子进程需要；首次传大，增量更新）"
rsync -avz --progress \
  --exclude='node_modules' \
  --exclude='dist' \
  --exclude='dist-ssr' \
  --exclude='.vite' \
  --include='*/' --include='src/**' \
  --include='package.json' \
  --include='package-lock.json' \
  --include='tsconfig.json' \
  --include='tsconfig.node.json' \
  --exclude='*' \
  "${ROOT}/upstream/" \
  "${SSH_HOST}:${SERVER_REPO}/upstream/"

# ─── 上传 server config + 注入真实 API key ──────────────────
# server.example.json 中 llmConfig.apiKey 写 PLACEHOLDER_FILL_ON_SERVER，
# 部署时用 sed 替换成真实密钥再上传，chmod 600 限制读取。
echo "==> 上传 server config（含真实 LLM_API_KEY，chmod 600）"
TMP_CONFIG=$(mktemp)
trap 'rm -f "$TMP_CONFIG"' EXIT
if ! grep -q 'PLACEHOLDER_FILL_ON_SERVER' "$CONFIG_LOCAL"; then
  echo "  警告: $CONFIG_LOCAL 不含 PLACEHOLDER_FILL_ON_SERVER，密钥将原样上传" >&2
  cp "$CONFIG_LOCAL" "$TMP_CONFIG"
else
  sed "s|PLACEHOLDER_FILL_ON_SERVER|${LLM_API_KEY}|g" \
    "$CONFIG_LOCAL" > "$TMP_CONFIG"
fi
rsync -avz --progress \
  "$TMP_CONFIG" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/config/server.local.json"
"${SSH[@]}" "$SSH_HOST" "chmod 600 ${SERVER_REPO}/overlay/config/server.local.json"

# ─── 服务端补装 Node 依赖（轻量） ──────────────────────────
echo "==> 服务端 npm ci overlay/cli/node（含 dev：tsx 是 runtime 依赖）"
"${SSH[@]}" "$SSH_HOST" "cd ${SERVER_REPO}/overlay/cli/node && \
  ([ -d node_modules/tsx ] && echo 'tsx already installed' || npm ci 2>&1 | tail -5)"

# ─── 服务端装 upstream/node_modules ──────────────────────────
# chat 子进程通过 @/ 别名解析到 upstream/src/，而那些文件会 import zustand/
# milkdown 等上游包。Node 解析器沿着目录向上找 node_modules，所以要装到
# upstream/ 下。
echo "==> 服务端 npm ci upstream/（chat 子进程需要）"
"${SSH[@]}" "$SSH_HOST" "cd ${SERVER_REPO}/upstream && \
  ([ -d node_modules/zustand ] && echo 'upstream deps already installed' || npm ci --omit=dev 2>&1 | tail -5)"

# ─── 验证产物是 musl 静态 ───────────────────────────────────
echo "==> 验证远端二进制是 musl 静态"
"${SSH[@]}" "$SSH_HOST" "file ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server | head -1"
"${SSH[@]}" "$SSH_HOST" "ldd ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server 2>&1 | head -3"
"${SSH[@]}" "$SSH_HOST" "file ${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki | head -1"

# ─── 写 systemd unit ───────────────────────────────────────
echo "==> 写 systemd unit"
"${SSH[@]}" "$SSH_HOST" "cat > /etc/systemd/system/llm-wiki-server.service" <<UNIT
[Unit]
Description=llm_wiki-server (HTTP read-only)
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=${SERVER_REPO}
Environment=LLM_WIKI_PROJECT=${SERVER_WIKI_ROOT}
Environment=LLM_WIKI_API_TOKEN=${SERVER_TOKEN}
Environment=LLM_WIKI_CONFIG=${SERVER_REPO}/overlay/config/server.local.json
Environment=LLM_WIKI_STATIC=${SERVER_REPO}/upstream/dist
Environment=LLM_WIKI_BIND=${SERVER_BIND}
Environment=LLM_WIKI_REPO=${SERVER_REPO}
ExecStart=${SERVER_REPO}/overlay/server/target/release/llm-wiki-server
Restart=on-failure
RestartSec=5
StandardOutput=append:/var/log/llm-wiki-server.log
StandardError=append:/var/log/llm-wiki-server.log

[Install]
WantedBy=multi-user.target
UNIT

# ─── 启动服务 ─────────────────────────────────────────────
echo "==> 启动服务"
"${SSH[@]}" "$SSH_HOST" "systemctl daemon-reload && \
  systemctl enable llm-wiki-server && \
  systemctl restart llm-wiki-server && \
  sleep 2 && \
  systemctl status llm-wiki-server --no-pager | head -15"

# ─── 验证 HTTP ─────────────────────────────────────────────
echo "==> 验证 HTTP API"
sleep 1
"${SSH[@]}" "$SSH_HOST" "curl -sS -H 'Authorization: Bearer ${SERVER_TOKEN}' \
  http://127.0.0.1:${SERVER_PORT}/api/v1/health && echo"
"${SSH[@]}" "$SSH_HOST" "curl -sS -H 'Authorization: Bearer ${SERVER_TOKEN}' \
  http://127.0.0.1:${SERVER_PORT}/api/v1/projects | head -c 400 && echo"

echo "==> 完成"
echo "  内网: http://127.0.0.1:${SERVER_PORT}/"
echo "  公网: 需配合 Cloudflare Tunnel（见 docs/部署-ECS与Tunnel.md）"
echo "  日志: ssh ${SSH_ARGS[*]} ${SSH_HOST} 'journalctl -u llm-wiki-server -f'"
