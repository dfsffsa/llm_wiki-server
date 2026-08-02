# Waffo Pancake 付款接入（DocuChat Pro 订阅）

> Spec: [docs/superpowers/specs/2026-08-02-waffo-payments-design.md](../docs/superpowers/specs/2026-08-02-waffo-payments-design.md)
> 外部资料: https://docs.waffo.ai/llms-full.txt

## 1. Dashboard 准备（一次性）

1. 注册商户: https://pancake.waffo.ai/merchant/auth/signin
2. 建 store（或复用现有）
3. **API & Development → Create API Key（选 Test）→ 立即下载私钥**（只显示一次）
4. Products → 建订阅产品「Pro Monthly」: billingPeriod=monthly, USD 19.00, taxCategory=saas; 复制 Product ID
5. Settings → Webhooks → 复制 **Test Webhook Public Key**
6. Settings → Webhooks → Add Webhook: URL `https://<域名>/api/v1/billing/webhook`（test），订阅 subscription.* 事件

## 2. 服务器配置

`server.local.json`（或环境变量）:

| 变量 | 值 |
|------|-----|
| `WAFFO_MERCHANT_ID` | `MER_...` |
| `WAFFO_PRIVATE_KEY` | 私钥 PEM（base64 或转义换行） |
| `WAFFO_PRO_PRODUCT_ID` | `PROD_...` |
| `WAFFO_WEBHOOK_PUBLIC_KEY` | Dashboard 复制的 Test/Prod 公钥 |
| `WAFFO_ENVIRONMENT` | `test` 或 `prod` |

示例 `billing` 块见 `overlay/config/server.example.json`。密钥经 `deploy-ecs.sh` 的 sed 注入 `server.local.json`（chmod 600），**不进 git、不打日志**。

> ⚠️ 未设置 `WAFFO_*` 环境变量时，`${WAFFO_*}` 占位符不会被替换，服务器会拒绝启用 billing 并打警告日志（见 `parse_billing_config`）。部署前务必 export 这四个变量。

## 3. 本地联调（webhook 回环）

- 用 cloudflared 或 ngrok 把本机 8080 暴露成公网 HTTPS，把该 URL 配成 Dashboard 的 test webhook
- Dashboard → Send Test Event 验证签名（若 401，见 §5 第一行）
- 测试卡: 成功 `4576 7500 0000 0110` / 拒绝 `4576 7500 0000 0220`

## 4. 验证流程

`./scripts/e2e-billing.sh`（需登录态 + 真实支付一步手动完成）。

## 5. 排错

| 现象 | 原因 | 处理 |
|------|------|------|
| webhook 401 验签失败 | signed-payload 构造或公钥环境不对 | 用 Dashboard Send Test Event 抓真实 `X-Waffo-Signature` 对拍；确认 Test/Prod 公钥与事件 mode 一致 |
| checkout 502 | 私钥无效 / 时间戳超前 >1min | 检查 `WAFFO_PRIVATE_KEY` PEM 格式；服务器 NTP 校准 |
| 产品不可见 | 未 `.publish()` 到 prod | test 环境用 test key；上线前 publish + 换 prod key |
| checkout 401 未登录 | 无 session cookie | 先 `/auth/login` |
