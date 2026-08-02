# Waffo Pancake 付款功能设计（DocuChat Pro 订阅）

> **性质**：已批准设计（2026-08-02）。实现前请先通读。
> **上游参考**：[商业化缺口与下一步.md](../../商业化缺口与下一步.md) §P0-4 最小计费/权益 —— 本设计落地该待办（用 Waffo Pancake 替代 Stripe）。
> **相关**：[2026-06-20-public-deploy-auth-design.md](./2026-06-20-public-deploy-auth-design.md)（认证/配额底座）
> **外部资料**：https://docs.waffo.ai/llms-full.txt（官方全量文档）、官方 skill：https://docs.waffo.ai/integrate/skill

---

## 1. 目标与形态

让定价页「立即订阅」真正跑通：用户自助订阅 Pro，付款成功自动升级权益（聊天配额解锁），webhook 驱动自动续费/取消降级。

| 决策项 | 结论 |
|--------|------|
| 付费形态 | **自助订阅 SaaS**（用户自己付费升级 Pro） |
| 收款渠道 | **Waffo Pancake**（AI 友好，商家记录模式，托管 checkout） |
| 接入架构 | **纯 Rust**（`rsa`+`sha2`+`base64` 手写签名/验签，零新增 Node 依赖） |
| Pro 权益（MVP） | **仅提升每日聊天配额**。语义搜索/导出/存储等营销文案暂不实做（后端不区分） |
| 免费配额 | **3 个/天**（对齐营销页；服务器默认 50 仅在未配 billing 时作为回退） |
| 环境 | 先 **test mode** 跑通全流程；Dashboard 准备步骤见 §8 |

## 2. 背景与现状

- 付款目前为零：`overlay/static/pricing/index.html` 的「立即订阅」是 `<a href="/register">`，无任何 checkout/webhook 代码。
- 可复用底座：
  - `overlay/auth/`：SQLite `users`（含 `is_admin`、`email_verified_at`）、`usage_daily(user_id, date, chat_count)`
  - `chat.rs:153-187`：对 cookie 用户原子扣减 `try_increment_usage`；限额是**单一全局常量** `state.daily_chat_limit()`（默认 50，Bearer 绕过）
  - `/auth/me` 返回 `usage:{used,limit,date}`；`/lite/` 侧栏有用量展示
  - `config.rs` 支持 `${VAR}` 环境变量展开（`expand_env_placeholders`）
  - 部署：`scripts/deploy-ecs.sh` 用 sed 注入密钥到 `server.local.json`（`chmod 600`）

## 3. Waffo Pancake 接入要点（来自官方文档）

- **认证**：API Key（RSA-SHA256 签名），headers `X-Merchant-Id` / `X-Timestamp` / `X-Signature`。签名算法：
  ```
  canonicalRequest = METHOD + "\n" + PATH + "\n" + TIMESTAMP + "\n" + SHA256_BASE64(BODY)
  signature = RSA-SHA256(canonicalRequest, privateKey)   # PKCS1v15
  X-Signature = Base64(signature)
  ```
  时间戳窗口：**5 分钟前 ~ 1 分钟前**（超前 >1 分钟即 401）。每把 key 绑定 test/prod，无需 `X-Environment`。
- **Store Slug**（客户端免签名）：**静默丢弃** `metadata`/`orderMerchantExternalId`/`withTrial` —— 无法做订阅→用户映射，故**不走**。必须 API Key 服务端建 session。
- **Checkout**：`POST /v1/actions/checkout/create-session` → 返回 `checkoutUrl`（托管支付页），重定向过去即可。支持 `buyerEmail`、`metadata`、`orderMerchantExternalId`、`successUrl`、`expiresInSeconds`(默认 2700s)。
- **Webhook**：header `X-Waffo-Signature: t=<timestamp>,v1=<signature>`（Stripe 风格信封，RSA-SHA256）。**验签公钥**：Dashboard→Settings→Webhooks 复制，**平台级**（每环境一把，test/prod 分开，所有 store 共享）。**必须读原始 body 验签，不能先 parse JSON。**
- **Webhook 事件**（对权益生效的）：
  | 事件 | 含义 |
  |---|---|
  | `subscription.activated` | 首次付款成功，订阅激活 |
  | `subscription.payment_succeeded` | 续费成功 |
  | `subscription.canceling` | 用户请求取消（当期仍有效） |
  | `subscription.uncanceled` | 撤销取消 |
  | `subscription.canceled` | 完全终止 |
  | `subscription.past_due` | 续费失败 |
  | `order.completed` | 一次性购买成功（MVP 不用） |
  | `refund.succeeded` / `refund.failed` | 退款 |
- **Webhook payload**（`data` 关键字段）：`orderId`、`buyerEmail`、`amount`(display string)、`productName`、`orderMetadata`(checkout 时传入的 metadata)、`orderMerchantExternalId`、`currentPeriodStart/End`（订阅事件）、`mode`("test"/"prod")。
- **重试**：非 2xx 时按 5min/30min/2h/8h/24h 重试 5 次 → 必须幂等。
- **测试卡**：成功 `4576 7500 0000 0110`；拒绝 `4576 7500 0000 0220`。
- **签名格式不确定性**：docs 只写明 Stripe 风格 `t,v1`，未逐字写 `v1` 的签名原文构造；实现时用 Dashboard **Send Test Event** 实测确认（预期 `v1 = RSA-SHA256(publicKey, t + "." + rawBody)` base64）。**此为本方案唯一不确定点，任何方案都需此验证。**

## 4. 权益模型（DB 改动）

`overlay/auth/src/schema.rs` 幂等迁移（沿用 `email_verified_at` 的 `ALTER TABLE` 模式）：

```sql
ALTER TABLE users ADD COLUMN plan TEXT NOT NULL DEFAULT 'free';        -- 'free' | 'pro'
ALTER TABLE users ADD COLUMN waffo_order_id TEXT;                       -- ORD_xxx
ALTER TABLE users ADD COLUMN pro_since INTEGER;
ALTER TABLE users ADD COLUMN plan_period_end INTEGER;                   -- 当前计费周期截止
```

新表（`CREATE TABLE IF NOT EXISTS`）：

```sql
CREATE TABLE IF NOT EXISTS waffo_webhook_events (
  event_id   TEXT PRIMARY KEY,     -- webhook 事件去重
  event_type TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

auth crate 暴露方法：`get_plan(user_id) -> Plan`、`set_plan(user_id, plan, order_id, period_end)`、`has_processed_webhook(event_id)`、`mark_webhook_processed(event_id, type)`。

## 5. 配置（`overlay/config/server.example.json` 新增 `billing` 块）

```json
"billing": {
  "waffoMerchantId": "${WAFFO_MERCHANT_ID}",
  "waffoPrivateKey": "${WAFFO_PRIVATE_KEY}",
  "storeId": "STO_...",
  "proProductId": "PROD_...",
  "webhookPublicKey": "${WAFFO_WEBHOOK_PUBLIC_KEY}",
  "environment": "test",
  "freeTierDailyLimit": 3,
  "proTierDailyLimit": 10000,
  "checkoutSuccessUrl": "https://www.sship.online/pricing?upgraded=1",
  "language": "zh-Hans"
}
```

- 密钥经 `${VAR}` 展开（`config.rs` 已有机制）；私钥 PEM 用 base64 或转义换行注入（参考 Waffo skill PEM 处理）。**私钥永不进 git、不打日志。**
- **向后兼容**：`billing` 块缺失/未启用 → 服务器完全按现状运行（全局 `LLM_WIKI_DAILY_CHAT_LIMIT`）。配了才启用 per-plan 限额与 checkout/webhook 路由。
- `environment: "test"` → checkout 走测试环境（key 本身已绑定环境，此值用于 webhook 公钥选择与日志）。

## 6. 新路由（`overlay/server/src/api/billing.rs`）

注册到 `api/mod.rs` / `server.rs` 路由表。

### 6.1 `POST /api/v1/billing/checkout`（需 Cookie 登录）

1. 解析 body `{ "plan": "pro" }`（MVP 仅支持 pro；非法值 400）
2. 若用户已是 pro 且订阅有效 → 直接返回 409 或返回现有订阅信息
3. 服务端签名调 Waffo `POST /v1/actions/checkout/create-session`（**注意：Waffo 无 `cancelUrl` 字段，取消由托管页原生处理**）：
   ```json
   {
     "productId": "<proProductId>",
     "productType": "subscription",
     "currency": "USD",
     "buyerEmail": "<user.email>",
     "successUrl": "<checkoutSuccessUrl>",
     "metadata": { "userId": "<user_id>" },
     "orderMerchantExternalId": "dllm-<user_id>-<ts>",
     "language": "<config.language 或省略>"
   }
   ```
4. 返回 `{ "ok": true, "checkoutUrl": "<...>" }`
5. 前端跳转 `checkoutUrl`（托管支付页）

错误：Waffo 非 2xx → 透传错误并 502；签名/密钥缺失 → 503 提示未配置。

### 6.2 `POST /api/v1/billing/webhook`（无 Cookie，签名验证）

1. 读**原始 body**（不 parse）+ `X-Waffo-Signature` header（`t=...,v1=...`）
2. 用 `webhookPublicKey`（PEM）验证 `v1` 是否等于 RSA-SHA256(`t + "." + rawBody`)；时间容差 5 分钟；失败 → 401 且**不处理**
3. 校验 `mode` 与配置 `environment` 一致（test/prod 公钥不同）
4. **幂等**：`event.id` 已处理 → 直接 200
5. 按事件更新权益（§6.3），`mark_webhook_processed`
6. 处理完成后回 **200**（tiny_http 同步模型；验签 + SQLite 落库都是快操作，同步处理即可。若未来引入慢逻辑再改异步）

### 6.3 事件→权益动作

用户映射：`data.orderMetadata.userId` 主键 → `users.id`；兜底 `data.buyerEmail` 精确匹配。

| 事件 | 动作 |
|---|---|
| `subscription.activated` | `plan=pro`，存 `waffo_order_id=data.orderId`，`pro_since=now`，`plan_period_end=data.currentPeriodEnd` |
| `subscription.payment_succeeded` | 若 order_id 匹配该用户 → 延长 `plan_period_end` |
| `subscription.canceling` | 保留 pro，仅标记（`plan_period_end` 不变，到期由 `subscription.canceled` 降级） |
| `subscription.canceled` | `plan=free`，清 `waffo_order_id/period_end` |
| `subscription.uncanceled` | 恢复 pro（重新置 `plan=pro` + period_end） |
| `subscription.past_due` | 保留 pro（宽限），`log::warn` 记录 |
| `order.completed` | 非订阅，MVP 只记日志 |
| `refund.succeeded` | 若对应订阅已退款且 `orderMetadata` 命中用户 → `plan=free` |

所有事件落库前先记 `waffo_webhook_events`（幂等键），处理失败不影响响应（记录异常，Waffo 会重试）。

## 7. 配额强制改动（`chat.rs` + `auth_routes.rs`）

- 新增 `resolve_daily_limit(user_id)`：billing 启用时 pro→`proTierDailyLimit`，free→`freeTierDailyLimit`；未启用→全局 `daily_chat_limit()`（向后兼容）。
- `chat.rs:158` 的 `let limit = state.daily_chat_limit() as i64` 改为按用户解析。Bearer 绕过逻辑不变。
- `/auth/me` 响应增加：`plan`、`planPeriodEnd`，`usage.limit` 用解析后的值。

## 8. 前端接线

### 8.1 定价页「立即订阅」

`overlay/static/pricing/index.html` 与首页内嵌定价区（`overlay/static/index.html` §pricing）：
- 按钮从 `<a href="/register">` 改为 `<button>` + JS：
  ```js
  const r = await fetch('/api/v1/billing/checkout', {
    method: 'POST', headers: {'Content-Type':'application/json'},
    body: JSON.stringify({plan:'pro'})
  });
  if (r.status === 401) location.href = '/login?next=/pricing';
  else if (r.ok) { const j = await r.json(); location.href = j.checkoutUrl; }
  else alert('无法创建订单，请稍后再试');
  ```
- 点击期间按钮禁用 + 「正在跳转支付…」文案。
- 已登录用户的「立即订阅」直达 checkout；未登录跳登录。

### 8.2 用量/套餐展示

- `/lite/` 侧栏 `sidebar-usage`：免费显示 `剩余 N/3`，Pro 显示徽章 + 周期截止。
- `/auth/me`（前端消费）：已含 `usage`，新增 `plan` 字段即可；账户区可选显示订阅状态。

## 9. 安全

- Webhook 验签用平台级公钥；公钥从 Dashboard 复制，经 env 注入。
- 私钥仅存在于服务器环境变量/`server.local.json`（chmod 600），不打日志。
- checkout 端点需有效会话（登录用户），防匿名刷单；可加简单限速。
- 服务器时钟需 NTP 同步（签名 1 分钟超前窗口）。
- 生产必须 HTTPS（已在 Cloudflare/Tunnel 后）。

## 10. 测试

### 单元测试（Rust，`billing.rs` 内 `#[cfg(test)]`）

1. canonical request 构造正确（METHOD/PATH/TIMESTAMP/BODY_HASH）
2. RSA 签名生成 + 公钥验证 roundtrip（生成 key 对自验）
3. webhook payload 解析（事件类型、`orderMetadata.userId`、`currentPeriodEnd`）
4. 事件→权益状态机（activated/payment_succeeded/canceling/canceled/uncanceled/past_due）
5. per-plan 配额解析：free=3 / pro=10000 / 未配 billing=全局
6. 幂等去重（同一 event_id 二次处理无副作用）

### E2E（test mode）

- 前置：§11 Dashboard 准备好测试产品 + webhook URL（本地开发用 cloudflared/ngrok 隧道，或直接打到线上 `https://www.sship.online/api/v1/billing/webhook`）
- 流程：登录 → `POST /api/v1/billing/checkout` → 得 `checkoutUrl` → 测试卡 `4576 7500 0000 0110` 完成 → webhook `subscription.activated` → `GET /auth/me` 显示 `plan=pro` → chat 限额 10000 → 取消测试 → `subscription.canceled` → plan=free
- 拒绝卡 `4576 7500 0000 0220` → 无 `subscription.activated`，权益不变
- 验证脚本沿用 `scripts/e2e-*.sh` 模式，新增 `scripts/e2e-billing.sh`

### 签名格式实测（实现第一步）

Dashboard → Send Test Event → 我们的 webhook 端点收到后验签成功即确认 `v1 = RSA-SHA256(t + "." + rawBody)`。若不符，调整 signed-payload 构造（最可能差异点是分隔符或是否含 header），再验证。

## 11. Waffo Dashboard 准备清单（测试环境，用户执行）

1. 注册商户：https://pancake.waffo.ai/merchant/auth/signin
2. 建 store（或复用现有）
3. **API & Development → Create API Key**（选 Test）→ **立即下载私钥**（不再显示第二次）
4. Products → 建订阅产品「Pro Monthly」，`billingPeriod=monthly`，`USD 19.00`，`taxCategory=saas`；记 Product ID
5. Settings → Webhooks → 复制 **Test Webhook Public Key**
6. Settings → Webhooks → Add Webhook：URL `https://<域名>/api/v1/billing/webhook`（test），订阅 `subscription.*` 事件
7. 用测试卡跑通；之后 publish 产品到 prod + 建 Prod API Key + 复制 Prod Webhook Public Key

## 12. 非目标（明确不做）

- 真·无限（字面不设上限）——用 10000 当作实际无限
- 语义搜索/导出/存储门控
- 账单历史、发票页、客户自助管理页（MVP 用 Waffo 托管 checkout + webhook 自动降级；取消入口后续再加 customer portal）
- 年付、试用、折扣
- 一次性产品（`order.completed` 只记日志）
- 多货币（固定 USD，与营销页一致）

## 13. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-02 | 初稿：基于官方 docs + skill 调研，用户确认自助订阅/Pro仅配额/免费3天/纯Rust/test mode |
