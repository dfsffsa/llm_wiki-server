# Auth 内核实现 Prompt

你要实现一个 Go 认证内核，定位是：

- 面向单体网站
- 默认使用服务端 session cookie
- 可嵌入业务应用
- SQLite 起步，可兼容 PostgreSQL
- 风格参考 `smhanov/auth`，但只做更干净的 v1

实现目录：

- 目标代码位于 `./boringauth`

你必须先阅读：

- `./AUTH_SYSTEM_DESIGN_FOR_LLM.md`
- `./AUTH_ANALYSIS.md`

## 硬性要求

1. 使用 Go。
2. 标准库优先，不要引入重型 Web 框架。
3. 采用分层结构：
   - `boringauth` 顶层 service/config/types
   - `core` 放密码、token、cookie
   - `httpapi` 放 HTTP handler
   - `middleware` 放 `RequireUser` / `OptionalUser`
   - `store` 放接口
   - `store/sqlite` 放 SQLite 实现
4. 默认使用服务端 session cookie，不使用 JWT。
5. API 统一 JSON。
6. 错误响应统一格式。
7. session token、reset token 必须高熵随机生成。
8. token 在数据库中只存 hash。
9. cookie 必须：
   - `HttpOnly`
   - `SameSite=Lax`
   - `Path=/`
   - HTTPS 下可设置 `Secure`
10. 认证内核只负责身份，不负责订阅、项目、团队等业务数据。

## v1 必做功能

1. `POST /auth/register`
2. `POST /auth/login`
3. `POST /auth/logout`
4. `GET /auth/me`
5. `POST /auth/forgot-password`
6. `POST /auth/reset-password`
7. `RequireUser` middleware
8. `OptionalUser` middleware
9. SQLite store
10. schema 自动初始化

## 允许的实现策略

- 可以使用 `database/sql`
- SQLite driver 不要硬编码绑定在库内部；调用方可自行 blank import 驱动
- 密码哈希如果不想依赖外部包，可以自己实现 PBKDF2-HMAC-SHA256
- 默认 `Mailer` 可以提供一个 `NoopMailer`
- 默认 `RateLimiter` 可以提供一个进程内内存实现

## API 约定

### `POST /auth/register`

请求：

```json
{
  "email": "user@example.com",
  "password": "secret123"
}
```

成功响应：

```json
{
  "user": {
    "id": 1,
    "email": "user@example.com",
    "email_verified": false
  }
}
```

### `POST /auth/login`

请求：

```json
{
  "email": "user@example.com",
  "password": "secret123"
}
```

成功响应：

```json
{
  "user": {
    "id": 1,
    "email": "user@example.com",
    "email_verified": false
  }
}
```

### `GET /auth/me`

未登录：

- `401`

已登录：

```json
{
  "user": {
    "id": 1,
    "email": "user@example.com",
    "email_verified": false
  }
}
```

### `POST /auth/logout`

成功：

```json
{
  "ok": true
}
```

### `POST /auth/forgot-password`

请求：

```json
{
  "email": "user@example.com"
}
```

始终返回：

```json
{
  "ok": true
}
```

### `POST /auth/reset-password`

请求：

```json
{
  "token": "raw-reset-token",
  "password": "new-secret123"
}
```

成功：

```json
{
  "user": {
    "id": 1,
    "email": "user@example.com",
    "email_verified": false
  }
}
```

## Store 接口要求

必须有事务接口，支持：

- 创建用户
- 查用户
- 创建/查找/删除 session
- 创建/查找/删除 reset token
- 删除某用户所有 session

## SQLite 特殊要求

初始化时执行：

- `PRAGMA journal_mode=WAL;`
- `PRAGMA synchronous=NORMAL;`
- `PRAGMA foreign_keys=ON;`
- `PRAGMA busy_timeout=5000;`

并创建：

- `users`
- `sessions`
- `password_reset_tokens`
- `oauth_accounts`

## 实现顺序

1. 先补齐类型、错误、配置
2. 再实现 core
3. 再实现 store 接口和 sqlite store
4. 再实现 service
5. 再实现 HTTP routes
6. 再实现 middleware
7. 最后补一个最小 example server

## 不要做的事

- 不要引入 JWT
- 不要引入 ORM
- 不要把订阅/plan 逻辑塞进 auth
- 不要把 handler 写成业务逻辑中心
- 不要用不安全的明文 token 存储

## 输出要求

1. 代码要能被人类直接继续开发。
2. 包结构清晰。
3. 关键安全选择要写简短注释。
4. 如果有暂未实现的扩展点，明确留 TODO，但不要阻断 v1 主流程。
