# 公网部署:登录认证、个人历史与用量限额 — Design Spec

> **Status:** Design approved (2026-06-20),pending implementation
> **Date:** 2026-06-20
> **范围:** 在 `overlay/` 内为 `llm-wiki-server` 增加最小可用的多用户系统(注册/登录/会话历史/用量限额)
> **依据:** `docs/低成本网站经验/AUTH_SYSTEM_DESIGN_FOR_LLM.md` 设计原则 + [smhanov/auth](https://github.com/smhanov/auth) 已验证实践,Rust 自研最小内核

## 1. 目标

把当前共享 token 的 `lite/` 问答页(`http://127.0.0.1:8080/lite/`)平滑地推到公网,允许第三方用户使用,但不让个别用户烧光 LLM 配额。

**必须做到:**
- 个人账号(邮箱+密码)注册/登录/登出
- 个人聊天历史在服务端持久化(替代当前 localStorage)
- 每用户每天 N 次 chat 限额
- 落地页 + 登录/注册/重置密码页
- lite 页移动端适配 + 用户栏 + 历史侧边栏
- 现有 Bearer token 鉴权保留,不破坏 e2e/CLI

**明确不做(YAGNI):**
- OAuth(Google/GitHub 等)
- SAML / SSO
- 邮箱验证(留 v1.1)
- 多设备 session 管理 UI
- 组织/团队权限
- 计费/订阅
- 复杂仪表盘

## 2. 非目标与边界

- **不重写 server**:沿用现有 Rust + tiny_http,新增模块,不引入新进程/新语言
- **不引入前端框架**:落地页 + 登录页用纯 HTML/CSS,lite 页延续原生 JS,符合"内容页无框架,工作台页才交互"的低成本网站原则
- **不做跨设备 SSO**:单产品认证,先打稳

## 3. 架构

```
浏览器
  │  (session cookie: HttpOnly + Secure + SameSite=Lax)
  ▼
llm-wiki-server  (Rust 单进程,沿用现有部署)
  ├─ overlay/auth/             ← 新增:认证内核
  │   ├─ mod.rs                 路由分发 + 中间件
  │   ├─ store.rs               SQLite (rusqlite)
  │   ├─ password.rs            argon2id
  │   ├─ session.rs             token 生成/校验/cookie
  │   ├─ ratelimit.rs           漏桶,user+ip 双维度
  │   └─ schema.sql             建表语句
  ├─ overlay/server/src/api/   ← 现有 API,加 auth 中间件 + 用量计数
  ├─ overlay/static/
  │   ├─ index.html             ← 新增:落地页
  │   ├─ auth/                  ← 新增:登录/注册/重置页
  │   └─ lite/                  ← 改造:历史侧边栏、用户栏、移动适配
  └─ ...
```

**关键决策对比** (smhanov/auth Go 实现 vs 本设计 Rust 实现)

| 项 | smhanov 原版 | 本设计 | 理由 |
|---|---|---|---|
| 密码哈希 | bcrypt | **argon2id** | OWASP 当前首选;`argon2` crate 成熟 |
| session token 存储 | 明文 | **sha256 hash** | 设计文档要求,防库泄露后被用 |
| cookie 属性 | HttpOnly + Secure | **HttpOnly + Secure + SameSite=Lax** | 加 SameSite 防 CSRF |
| 限流算法 | 漏桶,user+IP | 漏桶,user+IP | 1:1 照搬 |
| 数据库 | SQLite/Postgres | **SQLite + WAL** | 单文件,零运维 |
| 路由风格 | `/user/*` 表单 | **`/auth/*` JSON** | 设计文档推荐;前端友好 |
| OAuth/SAML | 有 | 无(YAGNI) | v1 不做 |

**为什么不直接用 smhanov/auth(Go):** 我们的 server 是 Rust,改 Go = 重写整个 server(HTTP 路由、SSE 流式、API、static 托管)。认证内核本质很简单(几张表+几个路由),Rust 自研成本 << 重写整个 server。我们复用 smhanov 的**设计模式与实现细节**(限流参数、cookie 策略、错误返回风格),不复用其代码。

## 4. 数据模型

SQLite 文件,默认路径 `/var/lib/llm-wiki/auth.db`(可由 `LLM_WIKI_AUTH_DB` 覆盖)。启动时 `CREATE TABLE IF NOT EXISTS` 自动建表。

PRAGMA(照搬设计文档 §13):

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA foreign_keys=ON;
PRAGMA busy_timeout=5000;
```

### 4.1 users

```sql
CREATE TABLE users (
  id            INTEGER PRIMARY KEY,
  email         TEXT UNIQUE NOT NULL,      -- 入库前小写归一
  password_hash TEXT NOT NULL,             -- argon2id
  display_name  TEXT,
  is_admin      INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL
);
```

`is_admin=1` 由配置 `LLM_WIKI_ADMIN_EMAIL` 决定:该邮箱注册时自动标记 admin。**不**用"首个注册用户即 admin"——防抢注。

### 4.2 sessions

```sql
CREATE TABLE sessions (
  token_hash    TEXT PRIMARY KEY,          -- sha256(token) hex
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER NOT NULL,
  user_agent    TEXT,
  ip            TEXT
);
CREATE INDEX idx_sessions_user ON sessions(user_id);
```

token 生成:`OsRng` 32 字节 → base64url(无填充)。客户端 cookie 持原始 token,库里只存 sha256 hash。

### 4.3 password_reset_tokens

```sql
CREATE TABLE password_reset_tokens (
  token_hash    TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at    INTEGER NOT NULL
);
```

1 小时有效,**单次使用**(校验后立即删除)。

### 4.4 conversations / conversation_messages

```sql
CREATE TABLE conversations (
  id            TEXT PRIMARY KEY,          -- uuid v4
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  project_id    TEXT NOT NULL,             -- 关联 wiki 项目 id
  title         TEXT NOT NULL,             -- 取首条 user 消息前 24 字
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX idx_conv_user ON conversations(user_id, updated_at DESC);

CREATE TABLE conversation_messages (
  id              INTEGER PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role            TEXT NOT NULL,            -- user / assistant
  content         TEXT NOT NULL,
  created_at      INTEGER NOT NULL
);
CREATE INDEX idx_msg_conv ON conversation_messages(conversation_id, id);
```

### 4.5 usage_daily

```sql
CREATE TABLE usage_daily (
  user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  date       TEXT NOT NULL,                 -- YYYY-MM-DD,UTC
  chat_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, date)
);
```

每发一次 chat,对应 `(user_id, today)` 行 `UPSERT` 自增。

## 5. HTTP API

全部 JSON 请求/响应。错误统一格式:

```json
{ "error": { "code": "invalid_credentials", "message": "邮箱或密码错误" } }
```

### 5.1 认证类(`/auth/*`)

| 方法 路径 | 鉴权 | 作用 |
|---|---|---|
| `POST /auth/register` | 公开 | 注册;限流 register:ip 10/小时;成功自动登录发 cookie |
| `POST /auth/login` | 公开 | 登录;限流 login:email + login:ip 各 25/小时 |
| `POST /auth/logout` | cookie | 删当前 session |
| `GET /auth/me` | cookie | 返回 `{user, usage:{used,limit,reset_at}}`,未登录 401 |
| `POST /auth/forgot-password` | 公开 | 限流 forgot:ip 10/小时;**无论邮箱是否存在统一返回 `{ok:true}`** |
| `POST /auth/reset-password` | 公开 | 用 token 重置密码;清该用户所有 session |

请求体示例:

```http
POST /auth/login
{ "email": "user@example.com", "password": "secret" }

→ 200
Set-Cookie: session=<token>; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000; Path=/
{ "user": { "id": 1, "email": "user@example.com", "display_name": null, "is_admin": false } }
```

### 5.2 会话历史类(`/api/v1/conversations*`)

| 方法 路径 | 作用 |
|---|---|
| `GET /api/v1/conversations` | 列出当前用户会话(`updated_at DESC`,默认上限 50) |
| `POST /api/v1/conversations` | 新建,body `{project_id, title}` |
| `DELETE /api/v1/conversations/{id}` | 删除(必须属当前用户) |
| `GET /api/v1/conversations/{id}/messages` | 取该会话所有消息 |
| `POST /api/v1/conversations/{id}/messages` | 追加消息(role+content) |

### 5.3 现有 API 的鉴权改造

`is_authorized()` 改为按顺序尝试:

1. **session cookie** → 命中即放行,把 user 存请求上下文
2. **`Authorization: Bearer`** → 与 `LLM_WIKI_API_TOKEN` 比对(现有逻辑保留,给 CLI/e2e 用)
3. 都失败 → 401

**`POST /api/v1/projects/{id}/chat` 增加用量计数:**
- 进入时若有 user(cookie 鉴权),先查 `usage_daily`,超额返回 429 `daily_limit_exceeded`
- 不超额则原子 UPSERT `chat_count + 1`,然后照常 spawn chat 子进程
- 走 Bearer 鉴权的请求**不计 user 用量**(无 user 概念,沿用旧行为)。这是有意设计:Bearer 是管理/CLI 通道,不在用户限额范围内,公网部署时该 token 不应分发给终端用户

## 6. 鉴权流程

```
浏览器(同源)
  ├─ /              → 静态落地页;HTML 加载完调 /auth/me 决定按钮跳 /lite/ or /login
  ├─ /login         → 静态登录页;提交 → POST /auth/login → 跳 /lite/
  ├─ /register      → 同上
  ├─ /reset-password → 静态页;凭邮件链接里的 token 提交新密码
  └─ /lite/         → init() 先调 /auth/me;401 立即 location.href='/login'
                      所有 API fetch 自动带 cookie

CLI / e2e 脚本(机器)
  └─ 任意 API + Authorization: Bearer <LLM_WIKI_API_TOKEN>
     不影响,不进入 user 上下文,不计用量
```

## 7. 前端

### 7.1 页面分层

| 路径 | 类型 | 实现 | 登录要求 |
|---|---|---|---|
| `/` | 落地页 | 纯 HTML+CSS,~20 行 JS 调 /auth/me | 否 |
| `/login`, `/register` | 表单页 | 纯 HTML+CSS+ fetch | 否,已登录则跳转 /lite/ |
| `/reset-password` | 表单页 | 同上 | 否 |
| `/lite/` | 工作台 | 现有原生 JS 模块 | **是** |

### 7.2 落地页(`overlay/static/index.html`)

- 站名 + 一句介绍
- 三张能力卡片(智能问答 / 全文搜索 / 知识图谱),纯 CSS Grid,不用 JS 框架
- "开始使用"按钮:JS 调 `/auth/me`,200 跳 `/lite/`,401 跳 `/login`
- 体积目标:HTML+CSS < 30KB,无第三方 JS

### 7.3 登录/注册页(`overlay/static/auth/`)

- email + password 输入,可同页 tab 切换登录/注册
- "忘记密码" → `/reset-password`
- 错误内联展示(对应后端错误码)
- 限流时显示"尝试过于频繁"

### 7.4 lite 页改造(`overlay/static/lite/`)

**新增:**
1. **历史侧边栏**:左侧列 `GET /api/v1/conversations`,点击切换/新建/删除
2. **顶部用户栏**:显示 email + 今日剩余额度 + 登出
3. **未登录拦截**:`init()` 调 `/auth/me`,401 跳 `/login`
4. **额度耗尽提示**:`used >= limit` 时禁用输入框,提示"今日额度已用完(N/N),明日重置"
5. **移动端适配**:`@media (max-width: 768px)` 侧边栏折叠为汉堡菜单

**改造:**
- `loadStore`/`saveStore`(localStorage)→ 调 `/api/v1/conversations*` API
- chat 流式逻辑不变(已稳),仅在前后调用 conversations API 维护历史

### 7.5 不做

- 不上 React/Vue 框架
- 不做 SSR / hydration
- 不做原生移动 App

## 8. 安全

### 8.1 密码与 session

- argon2id,参数 m=19456 KiB / t=2 / p=1(OWASP 推荐)
- session token:32 字节 OsRng,base64url 无填充;库存 sha256 hash
- reset token:同上,1 小时有效,单次使用

### 8.2 限流(漏桶,内存,user+IP 双维度)

| 操作 | 速率 | 时间窗 |
|---|---|---|
| login | 25 次 | 1 小时 |
| register(IP) | 10 次 | 1 小时 |
| forgot-password(IP) | 10 次 | 1 小时 |
| chat(用量) | `LLM_WIKI_DAILY_CHAT_LIMIT`,默认 50 | 每天(UTC) |

漏桶实现照搬 smhanov/auth/ratelimit.go 算法,Rust 改写。

### 8.3 防信息泄露

- forgot-password 不区分邮箱是否存在,统一 `{ok:true}`
- login 失败统一返回"邮箱或密码错误"
- reset token、password hash、原密码**不进 stdout/stderr/日志**

### 8.4 XSS / CSRF / 注入

- chat 历史落库后,渲染走 lite 页现有 marked + DOMPurify 流程
- `SameSite=Lax` 防绝大多数 CSRF
- 写操作(POST/DELETE)额外校验 `Origin` 头与 `Host` 同源(双保险)
- SQL 全部参数化(rusqlite `?`),零字符串拼接
- 输入校验:email 正则 + ≤256 字符;password ≥8 字符;title 截断 24 字符

### 8.5 部署侧(沿用 ECS+Tunnel 现有方案)

- 8080 仍只绑 127.0.0.1
- SQLite 文件权限 600
- cookie `Secure` 标志根据 `X-Forwarded-Proto: https` 判断

## 9. 错误处理

统一格式 + 错误码:

| 错误码 | HTTP | 场景 |
|---|---|---|
| `invalid_input` | 400 | 邮箱格式/密码长度等 |
| `email_already_exists` | 409 | 注册重复邮箱 |
| `invalid_credentials` | 401 | 登录失败(统一文案) |
| `not_authenticated` | 401 | 需登录但未登录 |
| `rate_limited` | 429 | 漏桶超限 |
| `daily_limit_exceeded` | 429 | 用量限额 |
| `invalid_reset_token` | 400 | reset token 无效或已用 |
| `expired_reset_token` | 400 | reset token 过期 |
| `internal_error` | 500 | 兜底 |

handler panic 由现有 `catch_unwind`(server.rs)兜住,不拖垮 server。

## 10. 配置项

新增环境变量(全部可选,有默认值):

| 变量 | 默认 | 说明 |
|---|---|---|
| `LLM_WIKI_AUTH_DB` | `./auth.db`(开发);生产建议 `/var/lib/llm-wiki/auth.db` | SQLite 文件路径 |
| `LLM_WIKI_REQUIRE_LOGIN` | `false` | true 时 lite 页强制登录 |
| `LLM_WIKI_DAILY_CHAT_LIMIT` | `50` | 每用户每日 chat 上限 |
| `LLM_WIKI_ADMIN_EMAIL` | (空) | 该邮箱注册时自动 admin |
| `LLM_WIKI_SESSION_TTL_DAYS` | `30` | session cookie 有效期(**绝对到期,不滑动续期**;到期重登) |
| `LLM_WIKI_API_TOKEN` | (现有) | Bearer token,**保留**给 CLI/e2e |

`LLM_WIKI_REQUIRE_LOGIN=false` 时:lite 页**不强制**登录,沿用旧共享 token 模式;本地开发/内网部署不受影响。仅公网部署时显式开启。

## 11. 测试

集成测试(`overlay/auth/tests/`),用 `tempfile` 临时 SQLite:

1. 注册成功 → `/auth/me` 返回用户
2. 重复邮箱 → 409 `email_already_exists`
3. 登录成功 → 拿到 session cookie
4. 错误密码 → 401 统一文案
5. 登出后 `/auth/me` → 401
6. forgot-password 不存在的邮箱也 200(统一响应)
7. reset token 重置后旧 session 失效
8. 登录限流:第 26 次错误密码 → 429
9. chat 用量超额 → 429 `daily_limit_exceeded`
10. SQLite 并发 8 线程同时登录,无 panic / 数据损坏
11. Bearer token 路径仍工作(回归现有 e2e)

## 12. 不做的扩展(留 v1.1+)

- 邮箱验证(注册后发链接,unverified 用户限制更严)
- Google OAuth 一键登录
- 多设备 session 管理 UI(`/auth/sessions` 列表 + 踢出)
- 修改邮箱/密码 UI
- admin 后台(看用户列表、调额度、封号)
- 用量月度统计

## 13. 部署影响

ECS 部署文档(`docs/部署-ECS与Tunnel.md`)新增步骤:

1. SQLite 文件目录:`mkdir -p /var/lib/llm-wiki && chown deploy: /var/lib/llm-wiki && chmod 700`
2. systemd EnvironmentFile 加 `LLM_WIKI_AUTH_DB`、`LLM_WIKI_REQUIRE_LOGIN=true`、`LLM_WIKI_ADMIN_EMAIL`
3. 备份策略:`/var/lib/llm-wiki/auth.db` 加入备份清单(轻量,定期 cp 即可)

## 14. 实施顺序(给后续 implementation plan 用)

1. `overlay/auth/` 模块:schema + store + password + session(无 HTTP)
2. ratelimit
3. `/auth/*` 路由,接入 server 路由分发
4. `is_authorized` 改造,加 cookie 路径
5. conversations 表 + `/api/v1/conversations*` API
6. chat handler 加用量计数
7. 落地页 + 登录/注册/重置页(纯静态)
8. lite 页改造(用户栏 + 历史侧边栏 + 移动适配)
9. 集成测试
10. 文档:`docs/部署-ECS与Tunnel.md` + `README` 更新

## 15. 参考资料

- `docs/低成本网站经验/AUTH_SYSTEM_DESIGN_FOR_LLM.md` — 设计原则母本
- `docs/低成本网站经验/AUTH_DECISION.md` — 选型决策
- `docs/低成本网站经验/SMHANOV_AUTH_CODE_ANALYSIS.md` — 上游代码分析
- `docs/低成本网站经验/FRONTEND_ANALYSIS.md` — 前端分层原则(内容页无框架,工作台 SPA)
- [smhanov/auth](https://github.com/smhanov/auth) — Go 上游(参考实现,不直接使用)
- `docs/部署-ECS与Tunnel.md` — 现有部署方案,本设计在此基础上叠加
