# 作者网站登录实现调研

调研日期：2026-04-15

研究目标：

- 作者提到的登录到底是不是“30 行 Go”
- 作者几个网站当前暴露出的登录形态是什么
- `smhanov/auth` 到底提供了多少现成能力
- 你如果想复用，应该怎么理解“30 行”的边界

## 1. 先说结论

结论非常明确：

`作者不是用 30 行 Go 从零手写了一个完整登录系统，而是把“业务接入认证”压缩到了接近 30 行；真正的认证能力由 smhanov/auth 这个库承担。`

更准确地说：

- “30 行”成立的前提是你直接复用他的 `auth` 库。
- 这个库已经内置了：
  - 用户表和 session 表
  - 密码登录
  - cookie session
  - 忘记密码
  - OAuth
  - SAML
  - 基础限流
- 你自己的应用只需要：
  - 打开数据库
  - 配 SMTP / OAuth 配置
  - `http.Handle("/user/", auth.New(...))`

所以它吸引人的地方，确实不是吹牛，但你要理解边界：

- `30 行` 是“接入代码”级别
- 不是“认证系统总实现”级别

## 2. `smhanov/auth` 是怎么设计的

从仓库 `auth.go`、`userdb.go`、`schema.go`、`doc.go` 可以明确看出它是一个完整的自托管认证模块。

### 2.1 接口挂载方式非常直接

官方 quick start 基本就是这几步：

1. 打开 SQLite / Postgres
2. 配置 `auth.DefaultSettings`
3. 创建 handler
4. 挂到 `/user/`

典型代码：

```go
db, err := sqlx.Open("sqlite3", "users.db")
if err != nil {
    log.Fatal(err)
}

settings := auth.DefaultSettings
settings.SMTPServer = "smtp.gmail.com:587"
settings.SMTPUser = "example@gmail.com"
settings.SMTPPassword = "app-password"
settings.EmailFrom = "MyApp <support@myapp.com>"

authHandler := auth.New(auth.NewUserDB(db), settings)
http.Handle("/user/", authHandler)

log.Fatal(http.ListenAndServe(":8080", nil))
```

这就是你说的“30 行 Go 很有吸引力”的来源。因为业务项目里，真正需要你写的集成代码确实很短。

### 2.2 它暴露的核心路由

从 `auth.go` 的 `ServeHTTP()` 可以看到标准路由：

- `/user/auth`
- `/user/create`
- `/user/get`
- `/user/signout`
- `/user/update`
- `/user/oauth/remove`
- `/user/oauth/add`
- `/user/forgotpassword`
- `/user/resetpassword`
- `/user/saml/metadata`
- `/user/saml/acs`
- `/user/oauth/login/{provider}`
- `/user/oauth/callback/{provider}`

这意味着：

- 前端完全可以不依赖复杂 SDK
- 只要按这些 HTTP 端点请求即可
- 大部分认证流程已经被固定成“后端约定接口”

### 2.3 它底层不是 JWT，而是传统 session cookie

从 `auth.go` 可以直接确认：

- 登录后会创建一个随机 `session` cookie
- cookie 名就是 `session`
- 过期时间是 30 天
- `HttpOnly: true`
- 请求为 HTTPS 时自动加 `Secure`

这套设计说明作者偏向：

- 服务端 session
- 简化前端安全处理
- 避免你在早期项目里自己处理 JWT 刷新、撤销、多端状态等复杂度

对单体网站来说，这非常合理。

### 2.4 数据模型很朴素，完全符合“低成本网站”思路

从 `schema.go` 可见默认表很少：

- `Users`
- `Sessions`
- `oauth`
- `PasswordResetTokens`
- `AuthSettings`

这背后的理念是：

- 认证只解决认证
- 不搞过度抽象
- 用户扩展资料应该由你的业务表自己去加

### 2.5 它有针对 SQLite 的现实补丁

这个点很关键。

`userdb.go` 里不是盲目吹 SQLite，而是明确写了：

- `Begin()` 对 SQLite lock 做了指数退避重试
- `Commit()` 也做了 lock retry

说明作者确实在真实生产环境里碰过 SQLite 的锁问题，并做了工程化补偿，而不是只停留在博客口号。

这反而增加了可信度。

## 3. 作者自己的网站现在看起来是怎么做登录的

## 3.1 WebSequenceDiagrams：明显存在一套成熟登录系统

直接看站点 HTML 和前端 bundle，可以确认几个事实。

### 证据 1：前端配置里启用了 Google 登录

首页 HTML 里有：

- `allowGoogleSignIn: true`
- `googleClientId: ...apps.googleusercontent.com`
- `signInMethod: "user-managed"`

这说明它至少支持：

- 账号体系
- Google 登录
- 用户自己管理账号

### 证据 2：前端 bundle 里暴露了完整用户接口

`websequencediagrams.com` 的前端 bundle 中可直接看到这些请求：

- `/users/authenticate`
- `/users/googleAuth`
- `/users/logout`
- `/users/info`
- `/users/forgotpassword`
- `/users/resetpassword`
- `/users/add`
- `/users/change`
- `/users/removeOauth`

还能看到：

- `XMLHttpRequest`
- `withCredentials = true`
- 依赖 cookie 维持登录态

这说明它当前线上实现大概率是：

- 传统服务端 session
- 前端 SPA 调认证接口
- 浏览器自动带 cookie

### 关键判断

`WebSequenceDiagrams 当前线上跑的接口命名，与 smhanov/auth 公开仓库的标准接口并不完全一致。`

例如：

- 线上是 `/users/authenticate`
- 新库标准是 `/user/auth`

- 线上是 `/users/info`
- 新库标准是 `/user/get`

- 线上是 `/users/add`
- 新库标准是 `/user/create`

这说明两种可能：

1. WebSequenceDiagrams 用的是 `auth` 的早期内部版本，后来作者把它整理成了开源版。
2. WebSequenceDiagrams 仍在用一套更老的自定义 auth API，而开源库是后续标准化抽离出来的版本。

我更倾向于第 2 种加第 1 种的混合：

- `auth` 明显继承了作者长期使用的认证思路
- 但线上老站未必已经完全迁移到开源库当前接口命名

也就是说：

`你应该把 smhanov/auth 理解为“作者内部成熟认证体系的公开整理版”，而不是严格等同于 WebSequenceDiagrams 当前线上后端代码。`

## 3.2 WebSequenceDiagrams 还有支付 / 订阅 / 文件系统能力

bundle 里还能看到：

- Stripe 脚本
- 订阅用户状态字段
- 文件列表、文件创建、项目共享、邀请成员等能力

这进一步说明：

- 作者不是做了个“只有登录”的最小 demo
- 而是做了一套“认证 + 订阅 + 文件/项目权限”产品骨架

换句话说，真正值得学的是：

`把登录当成公共基础设施做薄，把产品功能做厚。`

## 3.3 eh-trade：有登录状态管理，但公开首页证据不如 WSD 明确

从 `eh-trade.ca` 首页 HTML 可以看出：

- 站点是 SPA
- 前端 bundle 里有用户 store
- store 中有：
  - `userInfo`
  - `hasActiveSubscription`
  - `loginModalOpen`
  - `postLoginRedirect`
  - `showSignUpModal`

这说明：

- eh-trade 明确存在登录体系
- 也明确存在“订阅状态”概念
- 登录不是纯展示页假按钮

但从公开 bundle 可快速确认到的证据里，没有像 WSD 那样明显暴露出一组清晰的 `/users/*` 接口名。原因大概率有三种：

1. 请求层被封装到更深的 service 模块
2. 经过压缩后更难直接 grep
3. 站点可能已经换成另一套更贴近业务的 API 层

因此我能确认的是：

- eh-trade 有登录和订阅体系
- 但仅凭当前公开前端证据，不能百分之百断言它现在直接使用 `smhanov/auth` 当前公开接口

更稳妥的结论是：

`eh-trade 很可能沿用了同一套设计哲学：服务端持有用户与订阅状态，前端只维护轻量用户 store。`

## 4. 作者到底“怎么实现登录”的最可信复原

结合仓库和线上网站，我认为作者的方法论是这样的：

### 第 1 层：统一 auth 内核

作者有一套长期复用的认证内核，核心能力包括：

- email/password
- session cookie
- OAuth
- password reset
- SAML
- 基础限流
- SQLite / Postgres 兼容

这套内核现在以 `smhanov/auth` 的形式公开。

### 第 2 层：网站自己的业务外壳

每个站点在这套内核之外，会再包一层更贴近业务的 API 和返回结构，例如：

- 用户订阅状态
- premium 权限
- 文件列表
- 项目共享
- 计费信息

所以线上产品并不是完全裸用 `auth.New(...)` 的默认输出，而是可能：

- 包装 `GetInfo`
- 自定义返回字段
- 增加业务 API
- 保留旧接口兼容

### 第 3 层：前端非常薄

作者显然不走“大前端鉴权平台”路线，而是：

- 前端直接请求后端 auth 路由
- 靠 `session` cookie 维持状态
- 登录后调用“获取当前用户信息”接口

这种方案的好处：

- 简单
- 可审计
- 不需要前端存 token
- 对单域名单体网站特别省心

## 5. 你最应该注意的误区

### 误区 1：以为 30 行就等于功能很弱

不是。

这里的“30 行”是：

- 你的应用接入成本低
- 不是系统功能弱

实际上这个库已经够完整了。

### 误区 2：以为作者网站就是 100% 原样跑公开仓库

不能这么理解。

更像是：

- 作者线上站先有一套成熟方案
- 后来把其中较通用的部分抽成公开库

因此：

- 设计哲学高度一致
- 具体路由名和封装层未必完全相同

### 误区 3：以为“低成本”就应该用 JWT + 前后端分离

对作者这类站点来说，恰恰相反。

传统 session cookie 有几个现实优势：

- 实现更短
- 撤销会话简单
- 安全边界更清楚
- 单域名应用非常适合

如果你是做单体网站，JWT 往往只是在增加复杂度。

## 6. 如果你要模仿作者，我建议怎么做

### 最推荐的复用方式

如果你也想做低成本网站，我建议：

1. 直接把 `smhanov/auth` 当“认证子系统”
2. 挂到 `/user/`
3. 业务系统里只保留一个 `current_user` 获取接口
4. 前端只做：
   - 注册
   - 登录
   - 退出
   - 当前用户信息

### 最小可行接入方式

你完全可以按这个思路启动：

- SQLite
- `auth.New(auth.NewUserDB(db), settings)`
- `http.Handle("/user/", authHandler)`
- 自己业务 API 用 `auth.CheckUserID(...)` 或 `auth.GetUserID(...)`

这样你立刻获得：

- 账户创建
- 登录
- session cookie
- 忘记密码
- 用户读取

### 如果你要做付费产品

建议在 auth 之外额外建一张业务用户表，例如：

- `profiles`
- `subscriptions`
- `entitlements`

不要把订阅状态硬塞进 auth 表结构。

更好的方式是：

- auth 只管身份
- 业务表管权限和套餐

这和作者网站现有形态也更一致。

## 7. 最后的判断

我的综合结论是：

`作者的登录实现本质上是“传统单体网站认证工程化”，不是新潮 auth 平台化。`

他真正做对的地方有三点：

1. 用服务端 session，而不是把问题复杂化。
2. 把认证做成可复用内核，而不是每个项目重写。
3. 把业务接入成本压到极低，所以新项目看起来像“30 行就有登录”。

如果你想模仿他，这条路是靠谱的，而且非常适合低成本单体网站。

一句话总结：

`30 行 Go 吸引人的地方是真的，但前提是你复用了他背后那套已经写好的认证系统。`
