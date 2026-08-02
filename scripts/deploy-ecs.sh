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
#   # 全部参数走 env（含多用户认证 + 邮件）
#   SSH_HOST=... SSH_PORT=... SSH_CONFIG=... \
#   SERVER_REPO=/root/llm_wiki-server \
#   SERVER_WIKI_ROOT=/root/llm_wiki_projects \
#   SERVER_PORT=8081 \
#   SERVER_AUTH_DB=/var/lib/llm-wiki/auth.db \
#   SMTP_PASS='re_...' \
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
SERVER_AUTH_DB="${SERVER_AUTH_DB:-${SERVER_REPO}/auth.db}"  # 多用户认证 DB；默认跟随仓库，避免覆盖已有用户库
SERVER_REQUIRE_LOGIN="${SERVER_REQUIRE_LOGIN:-true}"
SERVER_ADMIN_EMAIL="${SERVER_ADMIN_EMAIL:-}"        # 该邮箱注册时自动 admin
SERVER_DAILY_CHAT_LIMIT="${SERVER_DAILY_CHAT_LIMIT:-50}"
SERVER_SESSION_TTL_DAYS="${SERVER_SESSION_TTL_DAYS:-30}"
SMTP_PASS="${SMTP_PASS:-}"                          # 可选；设置后注入 systemd 供 ${SMTP_PASS} 展开

# ─── 计算路径 ───────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SERVER_BIN_LOCAL="${ROOT}/overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server"
CLI_BIN_LOCAL="${ROOT}/overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki"
DIST_LOCAL="${ROOT}/upstream/dist"
CONFIG_LOCAL="${CONFIG_LOCAL:-${ROOT}/overlay/config/server.example.json}"   # 可覆盖为 per-server 真实配置

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
echo "  SERVER_AUTH_DB   = ${SERVER_AUTH_DB}"
echo "  SMTP_PASS        = $([[ -n "${SMTP_PASS}" ]] && echo "set(${#SMTP_PASS}c)" || echo "unset (email off)")"
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
  ${SERVER_REPO}/upstream \
  $(dirname "${SERVER_AUTH_DB}")"

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
# server.example.json 中 llmConfig.apiKey 写 PLACEHOLDER_FILL_ON_SERVER 或
# ${LLM_API_KEY} 占位符，部署时用 sed 替换成真实密钥再上传，chmod 600 限制读取。
echo "==> 上传 server config（含真实 LLM_API_KEY，chmod 600）"
TMP_CONFIG=$(mktemp)
trap 'rm -f "$TMP_CONFIG"' EXIT
if grep -qE 'PLACEHOLDER_FILL_ON_SERVER|\$\{LLM_API_KEY\}' "$CONFIG_LOCAL"; then
  sed -e "s|PLACEHOLDER_FILL_ON_SERVER|${LLM_API_KEY}|g" \
      -e "s|\${LLM_API_KEY}|${LLM_API_KEY}|g" \
    "$CONFIG_LOCAL" > "$TMP_CONFIG"
  echo "  已注入真实 LLM_API_KEY"
else
  echo "  警告: $CONFIG_LOCAL 不含任何密钥占位符，配置将原样上传" >&2
  cp "$CONFIG_LOCAL" "$TMP_CONFIG"
fi
rsync -avz --progress \
  "$TMP_CONFIG" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/config/server.local.json"
"${SSH[@]}" "$SSH_HOST" "chmod 600 ${SERVER_REPO}/overlay/config/server.local.json"

# ─── 上传 overlay/static/（公开落地页 + /lite/ QA 页）─────────
# systemd 的 LLM_WIKI_PUBLIC_LANDING_DIR 指向这里；缺失时远端会回退到
# 旧版/缺失的落地页。--delete 保证远端与本地一致（清除陈旧文件）。
echo "==> 上传 overlay/static/（公开落地页 + /lite/）"
rsync -avz --delete --progress \
  "${ROOT}/overlay/static/" \
  "${SSH_HOST}:${SERVER_REPO}/overlay/static/"

# ─── 上传 Node 依赖（从本机 rsync，不在远端 npm ci） ────────
# 1.6GB RAM 的远端跑 npm ci 会 OOM/超时；改成在本机装好 node_modules
# 再 rsync 过去。首次 ~559MB（upstream 524M + cli/node 35M），增量
# rsync delta ~10s。前提：本机已 npm ci 过 overlay/cli/node/ 和 upstream/。
echo "==> 检查本机 node_modules"
for d in "${ROOT}/overlay/cli/node/node_modules" "${ROOT}/upstream/node_modules"; do
  if [[ ! -d "$d" ]]; then
    echo "  缺少: $d" >&2
    echo "  请先在本机执行:" >&2
    echo "    npm ci --prefix ${ROOT}/overlay/cli/node" >&2
    echo "    npm ci --prefix ${ROOT}/upstream" >&2
    exit 1
  fi
done

echo "==> rsync overlay/cli/node/node_modules（含 tsx；约 35MB）"
rsync -avz --progress \
  --exclude='.cache' \
  "${ROOT}/overlay/cli/node/node_modules"/ \
  "${SSH_HOST}:${SERVER_REPO}/overlay/cli/node/node_modules/"

echo "==> rsync upstream/node_modules（约 524MB，首次较慢）"
rsync -avz --progress \
  --exclude='.cache' \
  --exclude='.vite' \
  "${ROOT}/upstream/node_modules"/ \
  "${SSH_HOST}:${SERVER_REPO}/upstream/node_modules/"

# ─── 验证产物是 musl 静态 ───────────────────────────────────
echo "==> 验证远端二进制是 musl 静态"
"${SSH[@]}" "$SSH_HOST" "file ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server | head -1"
"${SSH[@]}" "$SSH_HOST" "ldd ${SERVER_REPO}/overlay/server/target/release/llm-wiki-server 2>&1 | head -3"
"${SSH[@]}" "$SSH_HOST" "file ${SERVER_REPO}/overlay/cli/rust/target/release/llm-wiki | head -1"

# ─── 写 systemd unit ───────────────────────────────────────
# SKIP_SYSTEMD=1 时只生成 unit 文件到仓库目录（供无 sudo 的用户手动安装），
# 不写 /etc/systemd、不重启服务（例如 ecs199 的 li 用户需要 sudo 密码）。
UNIT_TARGET="/etc/systemd/system/llm-wiki-server.service"
if [[ "${SKIP_SYSTEMD:-0}" == "1" ]]; then
  UNIT_TARGET="${SERVER_REPO}/llm-wiki-server.service"
fi
echo "==> 写 systemd unit → ${UNIT_TARGET}"
"${SSH[@]}" "$SSH_HOST" "cat > ${UNIT_TARGET}" <<UNIT
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
Environment=LLM_WIKI_PUBLIC_LANDING_DIR=${SERVER_REPO}/overlay/static
Environment=LLM_WIKI_AUTH_DB=${SERVER_AUTH_DB}
Environment=LLM_WIKI_REQUIRE_LOGIN=${SERVER_REQUIRE_LOGIN}
$([[ -n "${SERVER_ADMIN_EMAIL}" ]] && echo "Environment=LLM_WIKI_ADMIN_EMAIL=${SERVER_ADMIN_EMAIL}")
Environment=LLM_WIKI_DAILY_CHAT_LIMIT=${SERVER_DAILY_CHAT_LIMIT}
Environment=LLM_WIKI_SESSION_TTL_DAYS=${SERVER_SESSION_TTL_DAYS}
$([[ -n "${SMTP_PASS}" ]] && echo "Environment=SMTP_PASS=${SMTP_PASS}")
ExecStart=${SERVER_REPO}/overlay/server/target/release/llm-wiki-server
Restart=on-failure
RestartSec=5
StandardOutput=append:/var/log/llm-wiki-server.log
StandardError=append:/var/log/llm-wiki-server.log

[Install]
WantedBy=multi-user.target
UNIT

if [[ "${SKIP_SYSTEMD:-0}" != "1" ]]; then
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
else
  echo "==> 文件部署完成（SKIP_SYSTEMD=1，未写 /etc、未启动）"
  echo "    手动安装 systemd（在目标服务器上执行，需 sudo）："
  echo "      sudo cp ${SERVER_REPO}/llm-wiki-server.service /etc/systemd/system/"
  echo "      sudo systemctl daemon-reload"
  echo "      sudo systemctl enable --now llm-wiki-server"
fi
