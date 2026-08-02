# Waffo Pancake 付款接入（DocuChat Pro 订阅）

> Spec: [docs/superpowers/specs/2026-08-02-waffo-payments-design.md](../docs/superpowers/specs/2026-08-02-waffo-payments-design.md)
> 实现计划: [docs/superpowers/plans/2026-08-02-waffo-payments.md](../docs/superpowers/plans/2026-08-02-waffo-payments.md)
> 外部资料: https://docs.waffo.ai/llms-full.txt

## 0. 当前状态（2026-08-02）

**代码已实现并提交 `main`**，测试全绿（server 57 / auth 54）。纯 Rust 接入，无新增 Node 依赖。

**已完成（可运行，但尚未真实收款验证）：**

| 模块 | 内容 |
|------|------|
| 加密 | `overlay/server/src/api/billing.rs`：RSA-SHA256 API 签名 + webhook 验签（`t.<rawBody>` 构造） |
| 权益 | `users` 表加 `plan`/`waffo_order_id`/`pro_since`/`plan_period_end` + `waffo_webhook_events` 幂等表 + `waffo_order_id` 唯一索引 |
| 路由 | `POST /api/v1/billing/checkout`（需登录）→ 建 Waffo 托管 checkout session；`POST /api/v1/billing/webhook`（验签→幂等→权益状态机→先回 200） |
| 权益状态机 | `subscription.activated/payment_succeeded/uncanceled`→授权；`canceling/past_due`→宽限；`canceled`→降级；`refund.succeeded`→**仅当退款订单==订阅订单**才降级 |
| 配额 | `chat.rs` 按 per-plan（free=3 / pro=10000）；`/auth/me` 返回 `plan.name`+`plan.periodEnd`；`/lite/` 侧栏 Pro 徽章 |
| 前端 | 定价页 + 首页「立即订阅」接 checkout（含弹窗拦截兜底 `if (!win) location.href=...`） |
| 配置/脚本 | `server.example.json` 加 `billing` 块；`scripts/e2e-billing.sh`（半自动）；本 runbook |

**尚未验证（需真实 Waffo 测试环境，见 §7）：**

1. **webhook 签名格式** —— 代码假定 `v1 = RSA-SHA256(t + "." + rawBody)`（Stripe 约定）。这是纯 Rust 方案唯一不确定点，必须用 Dashboard **Send Test Event** 对拍一次；若不符只需改 `verify_webhook_signature` 一处。
2. **真实 checkout → 测试卡支付 → webhook → plan 升级** 全链路（`e2e-billing.sh` 需先完成 Dashboard 准备）。

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
| `WAFFO_PRIVATE_KEY` | 私钥 PEM（标准 `-----BEGIN ...-----` 格式） |
| `WAFFO_PRO_PRODUCT_ID` | `PROD_...` |
| `WAFFO_WEBHOOK_PUBLIC_KEY` | Dashboard 复制的 Test/Prod 公钥 |

示例 `billing` 块见 `overlay/config/server.example.json`。`environment`（`test` 或 `prod`）直接在 `server.local.json` 的 `billing` 块里写死，无对应环境变量。以下值通过 `deploy-ecs.sh` 的 sed 注入到 `server.local.json`（非运行时环境变量，chmod 600），部署前需 export 这些变量供 deploy 脚本使用；**不进 git、不打日志**。

> ⚠️ 未设置 `WAFFO_*` 环境变量时，`${WAFFO_*}` 占位符不会被替换，服务器会拒绝启用 billing 并打警告日志（见 `parse_billing_config`）。部署前务必 export 这四个变量。

## 3. 本地联调（webhook 回环）

- 用 cloudflared 或 ngrok 把本机 8080 暴露成公网 HTTPS，把该 URL 配成 Dashboard 的 test webhook
- Dashboard → Send Test Event 验证签名（若 401，见 §5 第一行）
- 测试卡: 成功 `4576 7500 0000 0110` / 拒绝 `4576 7500 0000 0220`
- 若 SMTP 已配置，验证 token 会通过邮件发送，不会出现在服务器日志中——请查收邮件而非从日志提取。

## 4. 验证流程

`./scripts/e2e-billing.sh`（需登录态 + 真实支付一步手动完成）。

## 5. 排错

| 现象 | 原因 | 处理 |
|------|------|------|
| webhook 401 验签失败 | signed-payload 构造或公钥环境不对 | 用 Dashboard Send Test Event 抓真实 `X-Waffo-Signature` 对拍；确认 Test/Prod 公钥与事件 mode 一致 |
| checkout 502 | 私钥无效 / 时间戳超前 >1min | 检查 `WAFFO_PRIVATE_KEY` PEM 格式；服务器 NTP 校准 |
| 产品不可见 | 未 `.publish()` 到 prod | test 环境用 test key；上线前 publish + 换 prod key |
| checkout 401 未登录 | 无 session cookie | 先 `/auth/login` |
| checkout 409 already subscribed | 用户已是有效 Pro | 前端 JS 会透传 `error:"already subscribed"` 提示；续费场景可先取消再订 |

## 6. 开发经验与坑

> 写给后续接手的人 —— 这些是踩过的坑和确认过的行为，改代码前先看。

1. **Waffo 没有 `cancelUrl` 字段**，只有 `successUrl`；取消由托管支付页原生处理。早期设计文档写过 `cancelUrl`，已移除。别再加回去。
2. **checkout `metadata` 会透传到 webhook 的 `data.orderMetadata`** —— 用它传 `userId` 做「订阅→用户」映射（主键，`resolve_user_id` 第一优先），兜底 `buyerEmail`（须 `to_ascii_lowercase()` 匹配注册时规范化）。⚠️ Store Slug 认证会**静默丢弃** metadata/orderMerchantExternalId —— 必须走 API Key 服务端建 session。
3. **退款降级防误杀**：`refund.succeeded` 只当退款订单 == 用户的 `waffo_order_id`（订阅订单）才降级，绝不按 email 兜底降级（`process_webhook_event` 的 `DowngradeToFree` 分支区分了 event_type）。否则无关订单退款会误撤销 Pro。
4. **webhook 幂等是 check-then-record（best-effort）**：`has_webhook_event` → 处理 → `record_webhook_event`。**别改成 record-first**（`try_record_webhook_event` 已因死代码移除）——那会让处理失败的事件被幂等跳过，Waffo 重试也白搭。并发双投虽会重复处理，但 `set_plan` 是幂等 UPSERT，结果一致。
5. **rsa crate 默认 feature 够用**：`rsa = { version = "0.9", features = ["sha2"] }` 就带 pkcs1/pkcs8，`from_pkcs8_pem`/`from_pkcs1_pem` 都能用。Waffo 下发的私钥是 PKCS#1（`BEGIN RSA PRIVATE KEY`），走 `from_pkcs1_pem` 分支（测试 `sign_and_verify_with_pkcs1_pem` 覆盖）。
6. **时间戳窗口不对称**：API 签名超**前 1 分钟**即 401 —— 服务器时钟超前比滞后更危险，务必 NTP 校准；webhook 验签容差同此（`ts > now+60 || ts < now-300`）。
7. **`environment` 直接写死在 config**，没有 `${WAFFO_ENVIRONMENT}` 占位符；别在文档里当环境变量列。
8. **未设置的 `${WAFFO_*}` 占位符**会让 `parse_billing_config` fail-closed（拒绝启用 billing + 警告日志）。这是有意的：宁可账单关掉也不要拿字面量去签名拿 401。
9. **`iso_date_to_epoch` 校验日历合法性**：`2026-02-30` 会被拒（`days_in_month` 检查）；webhook `currentPeriodEnd` 是 ISO 日期（无时区），按 00:00 UTC 转 unix 秒。
10. **编译慢**：server crate 链 lancedb，全量 build 约 10+ 分钟；迭代用 `cargo test --manifest-path overlay/server/Cargo.toml billing`（增量很快）。
11. **`storeId` 配置字段可省略**：API key 绑定 store，`proProductId` 已唯一确定产品；checkout 请求体也不需要 storeId。
12. **免费/Pro 配额**：free=3（对齐营销页）、pro=10000（实际无限）。`/auth/me` 的 `limit` 与 chat 实际强制用的是同一个 `resolve_daily_limit`，不会不一致。
13. **`pro_since` 列已维护但未暴露**（`/auth/me` 不返回）；如需「会员时长」展示再补。

## 7. 下一步 / 上线检查清单

> 换电脑/新会话继续开发，从「§0 当前状态」和本清单开始。

- [ ] **Waffo Dashboard 准备**（§1）：注册商户 → 建 store → **API & Development 建 Test API Key 并立即下载私钥**（只显示一次）→ 建订阅产品「Pro Monthly」USD 19.00 taxCategory=saas → 复制 Product ID → Settings→Webhooks 复制 **Test Webhook Public Key** → Add Webhook: `https://<域名>/api/v1/billing/webhook`（test）
- [ ] **配服务器**（§2）：`server.local.json` 加 `billing` 块，export `WAFFO_MERCHANT_ID`/`WAFFO_PRIVATE_KEY`/`WAFFO_PRO_PRODUCT_ID`/`WAFFO_WEBHOOK_PUBLIC_KEY`
- [ ] **签名格式对拍**（关键）：本机用 cloudflared/ngrok 暴露 webhook，Dashboard **Send Test Event**，确认验签通过；若 401 抓真实 `X-Waffo-Signature` 与 `verify_webhook_signature` 对拍（预期 `t + "." + rawBody`）
- [ ] **全链路 E2E**：`./scripts/e2e-billing.sh`（需登录 + 手动一步）用测试卡 `4576 7500 0000 0110` 完成支付 → webhook → `/auth/me` plan=pro、limit=10000
- [ ] **拒绝路径**：测试卡 `4576 7500 0000 0220` 支付失败 → 无 `subscription.activated`，权益不变
- [ ] **取消降级**：手动取消订阅 → `subscription.canceled` → `/auth/me` plan=free
- [ ] **上线（prod）**：产品 `.publish()` → 建 Prod API Key + 复制 Prod Webhook Public Key → `environment` 改 `"prod"` → webhook URL 改正式域名
- [ ] （可选）`proTierDailyLimit` 是否需要字面无限；`freeTierDailyLimit` 与营销文案核对
