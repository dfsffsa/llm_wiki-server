# 可复用认证内核设计方案

目标：

- 参考 Steve Hanov 一系网站的认证设计思路
- 提炼成一套适合单体网站、低成本部署、可长期复用的认证内核
- 文档面向“后续交给大模型直接实现”
- 默认技术背景：Go 单体应用 + SQLite 起步 + 可升级 PostgreSQL

相关参考对象：

- `smhanov/auth`
- `websequencediagrams.com`
- `eh-trade.ca`

---

## 1. 设计目标

这套认证内核不是做成 Auth0/Clerk 那种平台，而是做成：

- 一个可以嵌入单体网站的 Go 包
- 一个默认使用服务端 session cookie 的认证系统
- 一个把常见认证脏活统一收口的复用内核

核心目标：

1. 接入成本低
2. 部署成本低
3. 默认安全合理
4. 对 SQLite 友好
5. 对业务系统侵入小
6. 能支撑“工具站 / 内容站 / 轻 SaaS / 订阅站”

非目标：

- 不做前后端完全分离 token 平台
- 不做复杂 IAM / RBAC 平台
- 不做企业级组织权限系统 v1
- 不做 OAuth provider 平台

---

## 2. 从作者网站抽出的设计原则

## 2.1 认证要是公共底座，不要散落在每个项目里

从 `smhanov/auth` 和作者多个站点可以看出，他的真正优势不是“某个登录页”，而是：

- 登录能力被抽成统一模块
- 每个产品只接入，不重写
- 用户认证和业务逻辑解耦

因此本方案必须做成：

- 一个独立 package
- 一套稳定 HTTP 路由
- 一个可替换 store 接口

## 2.2 默认用服务端 session，而不是 JWT 优先

作者的站点更像传统单体应用：

- 服务器创建 session
- 浏览器拿 `HttpOnly` cookie
- 前端只拿当前用户信息

这套方案适合我们的原因：

- 单域名单体最简单
- 会话撤销容易
- 不需要前端持久化 token
- 更符合“低复杂度”目标

## 2.3 认证内核只处理身份，不吞掉所有业务

`WebSequenceDiagrams` 和 `eh-trade` 都显示出一个共同点：

- 认证之外还有订阅、权限、文件、项目、团队等业务数据
- 这些不是 auth 内核本体，而是 auth 外的一层业务外壳

所以这里必须明确边界：

- auth 内核：只负责身份、会话、认证方法、密码重置
- 业务系统：负责套餐、订阅、项目、团队、资源权限

## 2.4 允许“很薄接入”，但内核要完整

作者的“30 行 Go”之所以成立，是因为：

- 内核功能足够完整
- 外部接入足够薄

所以我们也要追求这个形态：

- 外部只需 20-50 行配置和挂载
- 内核内部则完整处理 cookie、session、rate limit、reset token、OAuth 等

---

## 3. 推荐产品形态

建议把它做成一个 Go 模块，例如：

- 包名：`boringauth`
- 仓库结构尽量简单

建议目录结构：

```text
/auth
  /core
  /http
  /middleware
  /store
    /sqlite
    /postgres
  /oauth
  /mail
  /internal
```

更具体一点：

```text
/auth
  auth.go
  config.go
  errors.go
  types.go
  hooks.go
  /core
    password.go
    session.go
    tokens.go
    cookies.go
  /http
    handler.go
    auth_routes.go
    helpers.go
  /middleware
    require_user.go
    optional_user.go
  /store
    store.go
    models.go
    sqlite.go
    postgres.go
    schema_sqlite.sql
    schema_postgres.sql
  /oauth
    google.go
  /mail
    sender.go
```

---

## 4. v1 功能范围

先做最小但完整的 `v1`，不要直接追求 `smhanov/auth` 全量。

## 4.1 必做功能

1. Email/Password 注册
2. Email/Password 登录
3. 当前用户读取
4. 登出
5. 忘记密码
6. 重置密码
7. 服务端 session cookie
8. 基础登录限流
9. SQLite 默认实现
10. PostgreSQL 兼容接口

## 4.2 v1 不做

1. Facebook OAuth
2. Twitter/X OAuth
3. SAML
4. 团队/组织权限
5. 多租户隔离
6. 复杂审计后台
7. 细颗粒度权限系统

## 4.3 v1.1 可以追加

1. Google OAuth
2. Email verify
3. 多 session 查看/踢出
4. 用户修改邮箱/密码
5. 自定义 `GetCurrentUserInfo`

---

## 5. 核心抽象设计

## 5.1 Config

需要一个顶层配置对象：

```go
type Config struct {
    BaseURL string

    SessionCookieName string
    SessionTTL time.Duration
    SecureCookies bool

    PasswordResetTTL time.Duration

    Mailer Mailer

    PasswordHasher PasswordHasher

    RateLimiter RateLimiter

    GoogleOAuth *GoogleOAuthConfig

    Hooks Hooks
}
```

设计原则：

- 所有默认值都能工作
- 最少配置即可跑通 Email/Password
- 高级能力按需打开

## 5.2 Store 接口

必须把存储抽象清楚，便于 SQLite / Postgres 切换。

```go
type Store interface {
    Begin(ctx context.Context) (Tx, error)
}

type Tx interface {
    Commit() error
    Rollback() error

    CreateUser(email string, passwordHash string) (User, error)
    FindUserByEmail(email string) (User, error)
    FindUserByID(id int64) (User, error)
    UpdateUserPassword(userID int64, passwordHash string) error
    UpdateUserEmail(userID int64, email string) error

    CreateSession(userID int64, sessionToken string, expiresAt time.Time) error
    FindSession(token string) (Session, error)
    DeleteSession(token string) error
    DeleteSessionsByUserID(userID int64) error
    TouchSession(token string, lastUsedAt time.Time) error

    CreatePasswordResetToken(userID int64, tokenHash string, expiresAt time.Time) error
    FindPasswordResetToken(tokenHash string) (PasswordResetToken, error)
    DeletePasswordResetToken(tokenHash string) error
    DeleteExpiredPasswordResetTokens(now time.Time) error

    LinkOAuthAccount(userID int64, provider string, providerUserID string, email string) error
    FindUserByOAuth(provider string, providerUserID string) (User, error)
}
```

注意：

- token 在数据库中应尽量存 hash，不直接存明文
- SQLite 实现要考虑锁重试

## 5.3 AuthService

业务核心应该集中在一个 service 层，而不是散在 handler。

```go
type Service struct {
    cfg Config
    store Store
}
```

它负责：

- 注册
- 登录
- 登出
- 获取当前用户
- 发忘记密码邮件
- 重置密码
- OAuth 登录合并

这样 HTTP 只是薄适配层。

---

## 6. HTTP 路由设计

建议统一走 `/auth/*`，不要混用 `/user/*` `/users/*` 多种历史路径。

v1 推荐路由：

- `POST /auth/register`
- `POST /auth/login`
- `POST /auth/logout`
- `GET /auth/me`
- `POST /auth/forgot-password`
- `POST /auth/reset-password`

v1.1 可加：

- `GET /auth/oauth/google/start`
- `GET /auth/oauth/google/callback`
- `POST /auth/change-password`
- `POST /auth/change-email`

### 请求 / 响应风格

全部 JSON，不要 form-only。

例如：

`POST /auth/register`

```json
{
  "email": "user@example.com",
  "password": "secret"
}
```

响应：

```json
{
  "user": {
    "id": 123,
    "email": "user@example.com"
  }
}
```

同时写入 `HttpOnly` session cookie。

### 错误响应统一格式

```json
{
  "error": {
    "code": "invalid_credentials",
    "message": "Invalid email or password"
  }
}
```

推荐错误码：

- `invalid_input`
- `email_already_exists`
- `invalid_credentials`
- `not_authenticated`
- `rate_limited`
- `invalid_reset_token`
- `expired_reset_token`
- `internal_error`

---

## 7. 数据模型设计

## 7.1 users

```sql
users (
  id                integer / bigserial primary key,
  email             text unique not null,
  password_hash     text,
  email_verified    boolean not null default false,
  created_at        bigint not null,
  updated_at        bigint not null,
  last_seen_at      bigint not null
)
```

说明：

- 允许未来纯 OAuth 用户，因此 `password_hash` 可以为空
- `email_verified` 提前保留

## 7.2 sessions

```sql
sessions (
  token_hash        text primary key,
  user_id           integer not null references users(id) on delete cascade,
  created_at        bigint not null,
  last_used_at      bigint not null,
  expires_at        bigint not null,
  user_agent        text,
  ip                text
)
```

说明：

- 建议数据库内存 hash，不存原始 token
- 支持未来“设备会话管理”

## 7.3 password_reset_tokens

```sql
password_reset_tokens (
  token_hash        text primary key,
  user_id           integer not null references users(id) on delete cascade,
  created_at        bigint not null,
  expires_at        bigint not null
)
```

## 7.4 oauth_accounts

```sql
oauth_accounts (
  provider          text not null,
  provider_user_id  text not null,
  user_id           integer not null references users(id) on delete cascade,
  email             text,
  created_at        bigint not null,
  primary key (provider, provider_user_id)
)
```

---

## 8. Cookie 与 Session 策略

必须默认采用较保守安全策略。

Cookie 建议：

- `HttpOnly = true`
- `Secure = true` 在 HTTPS 环境默认开启
- `SameSite = Lax`
- `Path = /`
- Cookie 名默认：`session`

Session 策略：

- 默认有效期：30 天
- 每次认证通过后创建新 session
- 重置密码后删除其他 session，再签新 session
- 登出时仅删除当前 session

是否做 session sliding expiration：

- v1 可以不做复杂滑动续期
- 只更新 `last_used_at`
- 到期后强制重新登录

---

## 9. 密码策略

v1 不要搞过度复杂策略，但要有最低门槛。

建议：

- 最小长度 8
- 使用 `bcrypt` 或 `argon2id`
- 推荐优先 `argon2id`，但如果要先追求简单成熟，也可先 `bcrypt`

大模型实现时要明确：

- 不允许自定义弱 hash
- 不允许明文存储
- 不允许可逆加密替代 hash

---

## 10. 忘记密码流程

推荐流程：

1. 用户提交邮箱
2. 若邮箱存在，生成随机 reset token
3. 数据库存储 token hash + expiry
4. 发邮件给用户
5. 用户点击链接进入前端 reset 页面
6. 前端把 token + new password 提交到 `/auth/reset-password`
7. 服务端校验 token，更新密码，删除所有旧 session，创建新 session

安全要求：

- reset token 单次使用
- 有效期默认 1 小时
- 数据库存 hash，不存明文
- `forgot-password` 响应不要泄露“邮箱是否存在”过多细节

推荐返回：

```json
{ "ok": true }
```

即使邮箱不存在，也返回统一成功，减少枚举风险。

---

## 11. 当前用户与业务信息分层

这个地方要借鉴作者网站的真实产品形态。

认证内核默认只返回基础身份：

```json
{
  "user": {
    "id": 123,
    "email": "user@example.com",
    "email_verified": false
  }
}
```

业务站点再通过一层 adapter 扩展成：

```json
{
  "user": {
    "id": 123,
    "email": "user@example.com",
    "email_verified": true,
    "is_paid": true,
    "plan": "pro",
    "display_name": "Alice"
  }
}
```

所以要设计一个扩展点：

```go
type UserInfoResolver interface {
    ResolveUserInfo(ctx context.Context, tx Tx, user User) (any, error)
}
```

默认 resolver 返回基础信息。

这非常重要，因为：

- `WebSequenceDiagrams` 需要文件/订阅/权限信息
- `eh-trade` 需要订阅态
- 但 auth 本体不应该绑死这些字段

---

## 12. 中间件设计

至少提供两个中间件：

### 12.1 OptionalUser

- 若有合法 session，则把用户写入 context
- 若无 session，继续请求

适合：

- 首页
- 公共 API
- 可选个性化页面

### 12.2 RequireUser

- 必须有合法 session
- 否则返回 401

适合：

- 仪表盘
- 保存文件
- 订阅用户资源

建议 API：

```go
func (a *Auth) OptionalUser(next http.Handler) http.Handler
func (a *Auth) RequireUser(next http.Handler) http.Handler
func UserFromContext(ctx context.Context) (*User, bool)
```

---

## 13. SQLite 特殊策略

这部分必须明确交给大模型，不然容易写出“理论可用、生产脆弱”的版本。

要求：

1. 启用 WAL
2. 适当配置 busy timeout
3. 事务开始与提交对锁错误做有限重试
4. 清理任务尽量轻量
5. session / token 表建索引

推荐 pragma：

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA foreign_keys=ON;
PRAGMA busy_timeout=5000;
```

说明：

- 这正好和作者“SQLite 够用，但要做现实补丁”的思路一致

---

## 14. 可观测性与审计

v1 不做复杂审计平台，但至少保留事件 hook。

建议 hooks：

```go
type Hooks struct {
    OnUserRegistered func(ctx context.Context, user User)
    OnUserLoggedIn func(ctx context.Context, user User)
    OnUserLoggedOut func(ctx context.Context, userID int64)
    OnPasswordResetRequested func(ctx context.Context, email string)
    OnPasswordResetCompleted func(ctx context.Context, user User)
}
```

目的：

- 打日志
- 发欢迎邮件
- 同步业务表
- 做埋点

---

## 15. LLM 实现要求

这一部分是给后续大模型实现时的硬约束。

## 15.1 代码风格要求

- Go 标准库优先
- 不引入重型 Web 框架
- 允许使用 `sqlx`
- 保持单体可读
- 清晰的 service/store/handler 分层

## 15.2 安全要求

- 所有 session token 和 reset token 必须高熵随机生成
- 数据库存 token hash
- cookie 必须 `HttpOnly`
- 默认 `SameSite=Lax`
- 不能把密码 reset token 打到日志
- 不能把真实密码或 hash 暴露给响应

## 15.3 数据库要求

- 同时支持 SQLite 和 PostgreSQL
- schema 自动初始化
- SQLite 有 lock retry
- 所有用户查找应使用索引字段

## 15.4 API 要求

- 统一 JSON
- 错误格式统一
- handler 尽量薄
- service 层测试可独立运行

## 15.5 测试要求

至少要生成：

1. 注册成功
2. 重复邮箱注册失败
3. 登录成功
4. 错误密码失败
5. `/auth/me` 在登录后成功
6. 登出后 `/auth/me` 失败
7. 忘记密码生成 token
8. reset token 可重置密码
9. reset 后旧 session 失效
10. SQLite 并发下基本行为不崩

---

## 16. 面向产品站点的接入方式

这部分直接参考作者网站模式。

## 16.1 工具站模式（像 WebSequenceDiagrams）

适合：

- 文件保存
- 个人项目
- 分享链接
- 订阅升级

接入建议：

- auth 只管身份
- 业务表中加：
  - `projects`
  - `files`
  - `subscriptions`
- `/auth/me` 返回基础身份
- `/api/me` 返回业务组合态

## 16.2 订阅站模式（像 eh-trade）

适合：

- 会员内容
- 仪表盘
- 订阅权限 gating

接入建议：

- auth 管身份
- billing 模块管套餐和权限
- 前端 store 维护：
  - `user`
  - `has_active_subscription`
  - `post_login_redirect`

## 16.3 多站复用模式

如果未来你有多个站：

- 每个站独立部署 auth 内核实例
- 或者共享一个 auth 库，但不一定共享用户库

不建议 v1 做跨产品统一 SSO。
先把单产品认证打稳。

---

## 17. 推荐实现顺序

给大模型时，推荐按下面顺序实现：

1. 定义 `models`、`errors`、`config`
2. 定义 `Store` / `Tx` 接口
3. 实现 SQLite store 和 schema
4. 实现 password hash / token / cookie 工具
5. 实现 `Service`
6. 实现 HTTP handlers
7. 实现 `RequireUser` / `OptionalUser`
8. 写 integration tests
9. 再加 PostgreSQL
10. 最后再考虑 Google OAuth

不要倒过来。

---

## 18. 最终建议

如果目标是做出一个“类似 `smhanov/auth`、适合低成本单体网站”的认证内核，那么最重要的不是追求大而全，而是：

- 默认走服务端 session
- 默认先支持 Email/Password
- 默认 SQLite 可跑
- 给业务留清晰扩展点
- 把接入压缩到几十行

一句话版本设计原则：

`做一个薄接入、厚内核、强边界、偏单体的网站认证模块。`

这才是作者那套方案最值得复用的地方。
