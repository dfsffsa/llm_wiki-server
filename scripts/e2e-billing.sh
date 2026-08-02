#!/usr/bin/env bash
# Waffo Pancake 付款 E2E（test mode）。
# 依赖：服务器已带 billing 配置启动（test 环境）、可访问公网收 webhook。
# 用法: ./scripts/e2e-billing.sh [BASE_URL] [EMAIL] [PASSWORD]
set -euo pipefail

BASE="${1:-http://127.0.0.1:8080}"
EMAIL="${2:-billing-e2e@test.com}"
PASSWORD="${3:-longenoughpass}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say() { echo; echo "==> $*"; }
ok()  { echo "   ✅ $*"; }
fail(){ echo "   ❌ $*"; exit 1; }

say "Health check"
curl -sf "${BASE}/api/v1/health" >/dev/null || fail "server not up"
ok "server up"

say "Register + verify email (token 从服务器日志取)"
curl -sf -X POST "${BASE}/auth/register" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"password\":\"${PASSWORD}\"}" >/dev/null \
  || fail "register"
# e2e-auth 的 verify 逻辑：token 出现在服务器 stderr 日志（SMTP 未配时）
# 假设 SMTP 未配置（token 打到服务器 stderr 日志）。若 SMTP 已配置，token 会通过邮件发送，请从邮箱获取。
say "请在上一步服务器日志中复制 verify token，然后:"
read -rp "  粘贴 verify token: " VT
curl -sf -o /dev/null "${BASE}/auth/verify-email?token=${VT}" || fail "verify-email"
ok "email verified"

say "登录拿 cookie"
curl -sf -c "${TMP}/c.txt" -X POST "${BASE}/auth/login" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"password\":\"${PASSWORD}\"}" >/dev/null || fail "login"
ok "logged in"

say "创建 checkout session"
CHECKOUT="$(curl -sf -b "${TMP}/c.txt" -X POST "${BASE}/api/v1/billing/checkout" \
  -H 'Content-Type: application/json' -d '{"plan":"pro"}')" || fail "checkout 失败（非 2xx）"
echo "   checkoutUrl: $(echo "$CHECKOUT" | sed 's/.*"checkoutUrl":"\([^"]*\)".*/\1/')"
echo
echo "   👉 在浏览器打开上面的 checkoutUrl，用测试卡 4576 7500 0000 0110 完成支付"
echo "   👉 支付成功后 Waffo 会发 webhook 到服务器（需在 Dashboard 配好 test webhook URL）"
read -rp "   完成后按回车继续: " _

say "轮询 /auth/me 等待 plan=pro (最多 30s)"
for i in $(seq 1 15); do
  ME="$(curl -sf -b "${TMP}/c.txt" "${BASE}/auth/me")"
  PLAN="$(echo "$ME" | python3 -c 'import sys,json; print(json.load(sys.stdin)["plan"]["name"])')"
  LIMIT="$(echo "$ME" | python3 -c 'import sys,json; print(json.load(sys.stdin)["usage"]["limit"])')"
  [ "$PLAN" = "pro" ] && break
  sleep 2
done
[ "$PLAN" = "pro" ] || { echo "   (若超时，请检查 Waffo Dashboard 的 webhook 送达日志；可重新运行脚本轮询)"; fail "plan 未变 pro（当前: $PLAN）"; }
ok "plan=pro"
[ "$LIMIT" = "10000" ] && ok "limit=10000" || fail "limit 应为 10000, 实际 $LIMIT"

say "完成 ✅（取消订阅降级流程：Dashboard/客服取消 → webhook subscription.canceled → /auth/me plan=free，可手动复验）"
