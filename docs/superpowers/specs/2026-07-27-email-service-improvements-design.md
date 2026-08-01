# 邮件服务完善 — 设计文档

> **日期：** 2026-07-27
> **状态：** 草案
> **关联：** SMTP 配置见 [docs/邮件配置-SMTP-Resend.md](../../邮件配置-SMTP-Resend.md)

## 1. 目标

完善 llm_wiki-server 的邮件服务，使其适合作为正式网站部署。核心变更：

1. **注册邮箱验证** — 用户注册后必须验证邮箱才能登录
2. **欢迎邮件** — 验证成功后自动发送
3. **变更邮箱** — 旧邮箱发提醒 + 新邮箱发验证，两步确认
4. **HTML 邮件模板** — 纯文本升级为 multipart/alternative（HTML + 纯文本 fallback）

当前邮件服务只支持密码重置（单一纯文本模板）。

---

## 2. 数据模型变更

### `overlay/auth/src/schema.rs`

```sql
-- users 表增加一列
ALTER TABLE users ADD COLUMN email_verified_at INTEGER;

-- 邮箱验证 token 表（跟 password_reset_tokens 同模式）
CREATE TABLE IF NOT EXISTS email_verification_tokens (
  token_hash    TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at    INTEGER NOT NULL
);

-- 变更邮箱暂存表
CREATE TABLE IF NOT EXISTS pending_email_changes (
  id                INTEGER PRIMARY KEY,
  user_id           INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  new_email         TEXT NOT NULL,
  new_email_token_hash TEXT NOT NULL UNIQUE,
  new_email_expires_at INTEGER NOT NULL,
  old_email_token_hash  TEXT NOT NULL UNIQUE,
  old_email_expires_at  INTEGER NOT NULL,
  created_at        INTEGER NOT NULL
);
```

**设计说明：**
- `email_verified_at` 为 `INTEGER`（Unix timestamp），`NULL` 表示未验证
- 存量用户：首次启动时执行 `UPDATE users SET email_verified_at = created_at WHERE email_verified_at IS NULL AND created_at > 0`，不锁住已有用户
- `pending_email_changes` 用双 token：旧邮箱 token（确认通知）+ 新邮箱 token（验证新地址），两个都 valid 时才执行变更
- token 统一用 `generate_token()`（32 字节随机 + SHA-256 存 hash），与 `password_reset_tokens` 一致

---

## 3. Auth Service 变更

### `overlay/auth/src/error.rs` — 新增错误

```rust
pub enum AuthError {
    // ... 已有 ...
    EmailNotVerified,         // 登录时邮箱未验证
    EmailAlreadyVerified,     // 重复验证
    EmailChangeConflict(String), // 新邮箱已被其他账号使用
}
```

### `overlay/auth/src/service.rs` — 新增/变更方法

#### 3a. `register()` — 不再发 session

```rust
/// 创建用户但**不**签发 session。返回 User（其 email_verified_at = NULL）。
/// HTTP 层需在调用后立即发送验证邮件。
pub fn register(&self, input: RegisterInput<'_>) -> Result<User, AuthError> {
    // 同上：rate limit、验证、创建用户
    // 变更点：不调用 issue_session()
    // user.email_verified_at 保持 NULL
}
```

返回值从 `Result<AuthOutcome, AuthError>` 改为 `Result<User, AuthError>`（`AuthOutcome` 含 session token，不再需要）。

#### 3b. `start_verification()` — 生成验证 token

```rust
/// 为未验证用户生成邮箱验证 token。返回明文 token（HTTP 层构造链接）。
/// 已有有效 token 时复用（不重复生成）。
pub fn start_verification(&self, user_id: i64, now: i64) -> Result<String, AuthError> {
    // 检查 email_verified_at 是否已非 NULL → EmailAlreadyVerified
    // 检查是否已有未过期的 token → 复用
    // 否则生成新 token，存 email_verification_tokens 表
}
```

#### 3c. `complete_verification()` — 验证邮箱

```rust
/// 使用 token 完成验证。设置 email_verified_at = now。
/// token 单次使用（无论成败都删除）。
pub fn complete_verification(&self, token: &str, now: i64) -> Result<User, AuthError> {
    // 查 token → 校验过期 → 删 token → 设 email_verified_at
}
```

#### 3d. `login()` — 新增未验证检查

```rust
pub fn login(&self, input: LoginInput<'_>) -> Result<AuthOutcome, AuthError> {
    // 原有流程...
    let user = /* 查到的用户 */;
    // **新增：** if user.email_verified_at.is_none() {
    //     return Err(AuthError::EmailNotVerified);
    // }
    // ... 继续验证密码、发 session ...
}
```

#### 3e. `start_email_change()` — 发起变更邮箱

```rust
/// 用户发起邮箱变更。
/// - 检查新邮箱未被使用
/// - 生成两个 token：old_token（旧邮箱确认）、new_token（新邮箱验证）
/// - 存 pending_email_changes 表
/// - 返回 (old_token, new_token)
pub fn start_email_change(
    &self,
    user_id: i64,
    new_email: &str,
    now: i64,
) -> Result<(String, String), AuthError> {
    // 验证新邮箱格式、未占用
    // 生两个 token，存 pending_email_changes
    // 返回明文 token 对
}

/// 变更邮箱确认状态
pub enum EmailChangeStatus {
    /// 单边已确认，等待另一侧
    PendingOneSide,
    /// 双侧确认，邮箱已变更
    Completed,
}
```

#### 3f. `confirm_email_change()` — 完成变更邮箱

```rust
/// 使用新/旧邮箱的 token 完成变更。
/// 用户需先后（或任意顺序）提交两个 token 才生效。
/// 单 token 提交只记录"已确认"状态，双 token 齐全才执行。
pub fn confirm_email_change(
    &self,
    token: &str,
    now: i64,
) -> Result<EmailChangeStatus, AuthError> {
    // 查 pending_email_changes 中匹配 old_token_hash 或 new_token_hash
    // 标记对应 token 为"已确认"
    // 如果两个都确认了：更新 user.email = new_email，设 email_verified_at = now
    // 返回 EmailChangeStatus::PendingOneSide 或 EmailChangeStatus::Completed
}
```

---

## 4. 邮件层变更

### `overlay/server/src/mail.rs`

#### 4a. 通用发送函数

```rust
/// 通用 SMTP 发送。自动构建 multipart/alternative（HTML + plain text）。
pub async fn send_email(
    cfg: &SmtpConfig,
    to: &str,
    subject: &str,
    html_body: &str,
    plain_body: &str,
) -> Result<(), String> {
    // 构造 multipart 邮件：text/plain + text/html
    // 使用 lettre::message::MultiPart::alternative()
}
```

当前 `send_password_reset()` 改为调用 `send_email()`。

#### 4b. HTML 模板函数

全部为纯函数（no IO），返回 `String`。

```rust
// ========== 密码重置 ==========
pub fn build_reset_html(reset_url: &str) -> String { /* ... */ }
pub fn build_reset_plain(reset_url: &str) -> String { /* ...（已有） */ }

// ========== 邮箱验证 ==========
pub fn build_verify_html(verify_url: &str) -> String { /* ... */ }
pub fn build_verify_plain(verify_url: &str) -> String { /* ... */ }

// ========== 欢迎邮件 ==========
pub fn build_welcome_html(display_name: &str) -> String { /* ... */ }
pub fn build_welcome_plain(display_name: &str) -> String { /* ... */ }

// ========== 变更邮箱通知（旧邮箱） ==========
pub fn build_email_change_notice_html(confirm_url: &str) -> String { /* ... */ }
pub fn build_email_change_notice_plain(confirm_url: &str) -> String { /* ... */ }

// ========== 新邮箱验证 ==========
pub fn build_new_email_verify_html(verify_url: &str) -> String { /* ... */ }
pub fn build_new_email_verify_plain(verify_url: &str) -> String { /* ... */ }
```

HTML 模板风格：内联 CSS（兼容 Gmail/QQ/Outlook），简单布局（logo + 主要内容 + 底部），中文。

#### 4c. 高级发送函数

```rust
pub async fn send_verification_email(
    cfg: &SmtpConfig, to: &str, token: &str,
) -> Result<(), String> { /* 构造 URL → 调 send_email */ }

pub async fn send_welcome_email(
    cfg: &SmtpConfig, to: &str, display_name: &str,
) -> Result<(), String> { /* ... */ }

pub async fn send_email_change_notice(
    cfg: &SmtpConfig, to: &str, confirm_url: &str,
) -> Result<(), String> { /* ... */ }

pub async fn send_new_email_verification(
    cfg: &SmtpConfig, to: &str, verify_url: &str,
) -> Result<(), String> { /* ... */ }
```

---

## 5. HTTP 路由变更

### `overlay/server/src/api/auth_routes.rs`

#### 5a. `POST /auth/register` — 行为变更

```
请求:  POST /auth/register  { email, password }
响应:  200 { "ok": true, "message": "验证邮件已发送，请检查邮箱" }
       （不再返回 Set-Cookie）
流程:
  1. auth.register(input) → user（无 session）
  2. auth.start_verification(user.id, now) → token
  3. 解析 SMTP 配置
  4. 若有配置 → 发验证邮件
  5. 若无配置 → 打印 token 到日志（开发态）
```

#### 5b. `GET /auth/verify-email?token=xxx` — 新增

```
请求:  GET /auth/verify-email?token=xxx
响应:  302 Redirect → /login?verified=true （成功后）
       （或 /login?verified=failed）
流程:
  1. auth.complete_verification(token, now) → user
  2. 若有 SMTP → send_welcome_email(cfg, user.email, user.display_name)
  3. 重定向到 publicBaseUrl + "/login?verified=true"
```

`/login?verified=true` 将显示前端提示"邮箱已验证，请登录"。

#### 5c. `POST /auth/change-email` — 新增

```
请求:  POST /auth/change-email  { email: "new@example.com" }
       （需要登录 cookie）
响应:  200 { "ok": true, "message": "确认邮件已发送" }
流程:
  1. 验证 session
  2. auth.start_email_change(user.id, new_email, now) → (old_token, new_token)
  3. 构建旧邮箱确认链接 + 新邮箱验证链接
  4. send_email_change_notice(旧邮箱, 确认链接)
  5. send_new_email_verification(新邮箱, 验证链接)
```

#### 5d. `GET /auth/confirm-email-change?token=xxx` — 新增

```
请求:  GET /auth/confirm-email-change?token=xxx
响应:  302 Redirect → /settings?email=changed (完全确认后)
       或 render "请检查新邮箱" 页面（单边确认后）
流程:
  1. auth.confirm_email_change(token, now) → status
  2. 若 Completed → 重定向到设置页
  3. 若 Pending（另一侧未确认）→ 重定向到提示页
```

---

## 6. 前端变更

### React UI（`upstream/src/`）

路由和页面需要适配新流程：

| 页面 | 说明 |
|------|------|
| `/register` | 成功后展示"验证邮件已发送"提示页，而非直接进入应用 |
| `/login` | 支持 `?verified=true/failed` 参数显示提示 |
| `/settings` | 增加"修改邮箱"功能入口 |
| `/verify-email` | （可选）前端解析 URL token 调 API，或全部由服务端 302 处理 |

**建议：** 验证和变更邮箱跳转走服务端 302，避免前端空路由需要额外构建。服务端 `public_landing_dir` 的静态页面可以直接处理 `/login?verified=true` 的展示。

---

## 7. 错误处理

| 场景 | 用户看到 | HTTP 状态码 |
|------|----------|-------------|
| 未验证邮箱尝试登录 | "邮箱未验证，请先查收验证邮件" | 403 Forbidden |
| 重复验证 | "邮箱已验证" | 400 Bad Request |
| 验证 token 过期 | "验证链接已过期" | 410 Gone |
| 新邮箱已被注册 | "该邮箱已被使用" | 409 Conflict |
| 变更邮箱 token 过期 | "操作已超时，请重新发起" | 410 Gone |

---

## 8. 迁移策略

```sql
-- 首次部署时运行（放在 schema.rs 的 init_schema 中）
UPDATE users
  SET email_verified_at = created_at
  WHERE email_verified_at IS NULL AND created_at > 0;
```

- 首次启动自动执行
- 存量用户全部自动标记已验证
- 新注册用户 `email_verified_at` 为 `NULL`，直到完成验证

---

## 9. 测试计划

### 单元测试（`overlay/auth/`）

| 测试 | 层级 |
|------|------|
| `start_verification` 创建有效 token | service |
| `start_verification` 对已验证用户返回 `EmailAlreadyVerified` | service |
| `complete_verification` 正确标记时间戳 | service |
| `complete_verification` 双花攻击（重复使用 token）→ 失败 | service |
| `login` 未验证用户 → `EmailNotVerified` | service |
| `start_email_change` 新邮箱已被占用 → `EmailChangeConflict` | service |
| `confirm_email_change` 双 token 确认后更新 email | service |
| `confirm_email_change` 单 token 确认 → Pending 状态 | service |

### 单元测试（`overlay/server/` — mail.rs）

| 测试 | 层级 |
|------|------|
| HTML 模板含验证链接 | 纯函数 |
| HTML + plain 内容一致（标题、链接相同） | 纯函数 |
| `send_email` 构造的 MIME 含两个 part | 需集成或 mock |
| 多语言字符（中文）编码正确 | 纯函数 |

### 集成 / E2E

| 场景 | 方式 |
|------|------|
| 完整注册 + 验证 + 登录流程 | `e2e-full.sh` 扩展 |
| 变更邮箱 + 双 token 确认流程 | 新 e2e 脚本或 curl 步骤 |
| 无 SMTP 时验证 token 打印到日志 | `e2e-local.sh` 适配 |

---

## 10. 实施顺序

1. **数据层** — schema 变更 + store 方法 + migration
2. **Auth service** — 新方法 + login 检查 + 错误类型
3. **邮件层** — 通用 send + HTML 模板 + 新邮件函数
4. **HTTP 路由** — 注册行为变更 + 新 handler
5. **前端适配** — 提示页、设置页
6. **测试** — 单元 + 集成
7. **E2E 验证**

---

## 11. 未纳入范围

以下功能明确不做（与本次需求无关）：

- ❌ 邮件发送队列/重试（lettre 直连，失败即报错）
- ❌ 多语言模板（仅中文）
- ❌ 邮件发送历史记录
- ❌ 管理后台手动触发验证邮件
- ❌ 定期清理过期 token（SQLite 查询时过滤 expires_at 即可，不影响性能）
