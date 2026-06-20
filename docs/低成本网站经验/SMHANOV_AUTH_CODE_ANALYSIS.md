# `smhanov/auth` 代码分析

更新时间：2026-04-16

本文件基于本地拉下来的仓库代码分析，源码出处：

- 上游仓库：<https://github.com/smhanov/auth>
- Go 文档：<https://pkg.go.dev/github.com/smhanov/auth>
- License：MIT（见上游仓库 `LICENSE`）

> 原本地副本路径 `~/hn_low_cost_site_analysis_2026-04-15/smhanov-auth/` 在这次归档时未一并提交（保留出处即可，无需冗余复制开源代码）。如需阅读源码，直接 `git clone https://github.com/smhanov/auth` 即可。

这份分析的目标不是重复 README，而是回答几个更关键的问题：

- 这是不是一个“真能用”的认证库，还是只是作者放出来的示例？
- 它的架构、数据模型、默认接口到底是什么样？
- 哪些地方设计得很稳，哪些地方比较 old-school 或需要我们包一层？
- 如果我们后续采用它，最合理的接入姿势是什么？

## 1. 总判断

结论很明确：`smhanov/auth` 是一个完整、自托管、偏传统服务端风格的 Go 认证库，不是一个 30 行示例。

“30 行 Go”更准确地说，是业务项目的接入成本可以接近 30 行；真正的认证内核本身并不小。仓库里已经包含：

- Email/Password 注册登录
- 服务端 session cookie
- 忘记密码 / 重置密码
- 更新邮箱 / 更新密码
- OAuth：Google / Facebook / Twitter
- SAML SSO
- SQLite / PostgreSQL schema
- SQLite 锁冲突重试
- 基础 rate limit
- 一批真实测试

所以它和我们之前本地做的 `boringauth` 骨架不是一个层级的东西。它已经是“可用内核”，我们本地那版更像思路验证和 API 形状草稿。

## 2. 这个库的整体形态

它的使用方式很直接：

```go
db, _ := sqlx.Open("sqlite3", "mydatabase.db")
http.Handle("/user/", auth.New(auth.NewUserDB(db), settings))
```

也就是说，它不是让你东拼西凑几个 middleware，而是直接给你一个完整的 `http.Handler`，挂到 `/user/` 前缀下。

这个设计非常符合作者一贯风格：

- 单体应用优先
- 直接挂到现有 Go Web 项目里
- 不追求高度抽象的身份平台
- 先把 80% 场景用最少集成成本解决掉

## 3. 代码结构与核心抽象

从代码看，核心分成 4 层。

### 3.1 `Handler`

`auth.New(db, settings)` 返回一个 `Handler`，它负责：

- 路由分发
- 表单参数读取
- session cookie 写入
- 登录状态读取
- 调用 `DB` / `Tx`
- OAuth / SAML flow 编排
- 错误返回

默认路由全部由 `ServeHTTP` 自己处理，主要包括：

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
- `/user/oauth/login/twitter`
- `/user/oauth/callback/twitter`
- `/user/oauth/login/google`
- `/user/oauth/callback/google`
- `/user/oauth/login/facebook`
- `/user/oauth/callback/facebook`

这里还有一个很实用的小细节：它会把 `/users/` 自动归一化到 `/user/`。这和我们之前在 `WebSequenceDiagrams` 前端 bundle 里观察到的 `/users/*` 风格接口是能对上的，说明作者自己的实际项目里很可能长期存在过不同命名风格。

### 3.2 `DB` / `Tx` 接口

这个库虽然是完整 handler，但并没有把存储写死。

它定义了：

- `DB`
- `Tx`

`DB.Begin(ctx)` 返回事务对象 `Tx`，大多数真实操作都通过 `Tx` 完成，比如：

- 创建密码用户
- 根据 email / oauth id 查用户
- 读写密码
- 创建 password reset token
- 创建 / 删除 session
- 更新密码、更新邮箱
- 查询 / 绑定 / 移除 OAuth 方法
- 读写 SAML 配置

这意味着它不是“只有一个 sqlite demo”，而是已经把“认证流程”和“存储实现”拆开了。

### 3.3 `UserDB`

仓库自带的默认存储实现是 `userdb.go` 里的 `UserDB`，基于：

- `database/sql`
- `sqlx`
- SQLite / PostgreSQL

`NewUserDB(db *sqlx.DB)` 会自动建表，这很符合作者“低接入成本”的风格。

### 3.4 `Settings`

`Settings` 是这个库最关键的配置入口。里面包含：

- SMTP 配置
- 自定义 forgot password 邮件模板
- 自定义发信函数 `SendEmailFn`
- `OnAuthEvent`
- 自定义密码哈希 / 比对函数
- Google / Facebook / Twitter OAuth 配置
- 默认 context

这意味着这个库虽然外观看起来“很整套”，但其实已经预留了有限、但很够用的扩展点。

## 4. 它的认证模型是什么

### 4.1 核心是服务端 session，不是 JWT

它的登录模型是非常传统的：

- 登录成功后生成随机 session cookie
- cookie 名字默认就是 `session`
- session 存在数据库里
- 后端每次根据 cookie 查当前用户

这套模型的优点很明确：

- 很适合单体网站
- 逻辑简单
- 失效控制容易做
- 不需要前端处理 token 刷新

这和作者几个站点的整体风格完全一致：传统 Web 应用优先，不为了“现代感”引入 JWT 复杂度。

### 4.2 session 生命周期

从代码和 schema 看：

- session 默认保留 30 天
- `Sessions` 表里记录 `cookie`、`userid`、`lastUsed`
- 有 maintenance 逻辑清掉过期 session

更重要的是，测试明确覆盖了一个非常正确的安全行为：

- 修改密码时，旧 session 失效
- reset password 后，旧 session 失效
- 当前请求会收到一个新的 session cookie

这说明作者不是只做“能登录”，而是确实考虑了被窃 session 的失效问题。

## 5. 默认接口风格：不是 JSON-first，而是传统表单风格

这是我们采用时必须清楚的一点。

这个库的 handler 更接近传统后端表单接口，而不是现代前后端分离的 JSON API。

例如：

- `/user/create` 读 `email`、`password`、可选 `signin`
- `/user/auth` 读 `email`、`password`
- `/user/update` 读 `current_password`、`password`、`email`
- `/user/forgotpassword` 读 `email`
- `/user/resetpassword` 读 `token`、`password`

虽然返回里会有 JSON，但它的整体交互哲学仍然是“服务端 Web auth handler”。

这也是为什么我认为后续如果我们采用它，最合理的方式不是重写一套 auth，而是：

- 保留它作为内核
- 外面再包一层薄的 `/auth/*` JSON adapter

## 6. 默认返回的用户信息

`UserDB.GetInfo()` 默认返回的结构里包含：

- `userid`
- `email`
- `settings`
- `methods`
- `newAccount`

这里有一个非常重要的设计点：

业务侧可以通过 `DB.GetInfo(tx, userid, newAccount)` 覆盖这个返回。

也就是说，这个库的作者明确把“认证”和“业务用户补充信息”分开了：

- 认证库负责用户身份、session、密码、OAuth 等
- 业务系统负责返回订阅、配额、profile 等额外字段

这和作者其他项目的形态很一致，非常适合小团队和独立开发者。

## 7. SQLite 支持做得怎么样

这部分做得比一般开源 auth 库更接地气。

### 7.1 SQLite 是一等公民

仓库不是“顺手支持 SQLite”，而是真正按 SQLite 的运行现实做了处理。

例如 `UserDB.Begin()` 和 `Commit()` 都有针对 SQLite `database is locked` 的重试逻辑：

- `Begin()` 最多重试 5 次
- `Commit()` 最多重试 3 次
- 带退避等待

这非常像作者自己长期在低成本 VPS、单机数据库场景里踩出来的实现。

### 7.2 自动 maintenance

它还会顺手做维护工作：

- 清理 30 天前的 session
- 清理过期 password reset token

这也很符合“单体应用低维护成本”的思路。

### 7.3 对我们的意义

如果你未来的站点是：

- 单 VPS
- Go 单体
- SQLite 起步
- 流量不算特别夸张

那这套方案是非常匹配的。

## 8. OAuth 实现到底是怎样的

这部分已经不是 README 级别的“支持 OAuth”，而是真有完整 callback 流程。

### 8.1 Google

Google 登录实现里已经有：

- 标准 OAuth2 code flow
- `state` cookie
- PKCE verifier
- callback 里换 token
- 拉取 Google user info
- 已登录用户可绑定 Google 方法
- 未登录用户可直接登录 / 创建账号
- 完成后重定向到 `next`

这说明这不是“前端拿 token 发回来”的模式，而是标准的服务端 OAuth flow。

### 8.2 Twitter

Twitter 实现也比较完整：

- code flow
- PKCE
- `state` cookie
- `next` 跳转
- callback 拉取 `users/me`

但这里有一个值得注意的现实妥协：

- Twitter 取 email 并不总是稳定可得
- 库里为缺失 email 的场景做了 fallback
- 会构造类似 `username@twitter.example.com` 这样的占位 email

这说明作者的“用户主键模型”本质上仍然围绕 email 在转，OAuth 只是补充身份来源。

### 8.3 Facebook

Facebook 的处理也类似：

- 标准 code flow
- `state` cookie
- callback 拉取 `/me?fields=id,name,email`

如果 Facebook 没返回 email，库也会构造 fallback email。

### 8.4 OAuth 的整体策略

OAuth 登录成功后，库的逻辑是：

1. 先看 OAuth foreign id 有没有已绑定用户
2. 没绑定的话，再看 email 是否能匹配已有账号
3. 再不行，就创建一个新账号
4. 然后把该 OAuth 方法挂到用户上

这是非常务实、也非常适合单体产品的实现。

## 9. SAML 支持比预期更完整

SAML 不是一个“摆设接口”，而是真用了 `crewjam/saml` / `samlsp`。

代码里做了这些事：

- 首次初始化时自动生成 RSA 私钥和证书
- 保存到数据库里
- 可以导出 SP metadata
- 可以根据数据库里保存的 IdP metadata 发起登录
- 在 ACS 回调中解析 assertion
- 从常见字段中提取邮箱
- 找不到用户则自动创建
- 成功后建立本地 session

这说明它不仅面向普通独立站，也考虑到了“以后某个项目突然需要企业登录”的场景。

但也正因为如此，这个库天然会比“极简 auth”更厚一点。

## 10. Rate limit 做得怎么样

它有内置 rate limit，但定位很清楚：够用，不是分布式安全系统。

特点：

- 内存态实现
- 单进程级别
- 可按操作、IP、email 做限制
- 主要用于登录等敏感接口

这对于单机单体是合理的。

但如果以后系统变成：

- 多实例
- 多机部署
- 需要统一限流

那就要把这一层替换掉，或者在外层交给网关 / Redis。

## 11. 从测试里能确认什么

虽然当前环境没有 `go` 命令，我无法本地执行 `go test ./...`，但仓库内已有的测试文件已经能说明不少问题。

公开测试覆盖至少包含：

- 使用示例
- OAuth callback hook
- session invalidation
- Twitter / Google / Facebook flow 相关测试

其中最值得注意的是 `session_invalidation_test.go`，它明确验证了：

- 修改密码后旧 session 失效
- reset password 后旧 session 失效
- 新密码可以登录，旧密码不能再用

这属于“auth 内核里真正重要的细节”，不是样板代码会顺手做的东西。

## 12. 这个库强在哪里

如果从工程价值看，我认为它最强的是 6 点。

### 12.1 接入极薄

业务方可以非常低成本挂上：

- 一个 handler
- 一个数据库
- 一些配置

### 12.2 模型传统、稳定

它不是在追“新潮认证架构”，而是服务端 session + 数据库会话。这对绝大多数独立网站更稳。

### 12.3 SQLite 现实主义

很多库嘴上支持 SQLite，实际上没处理锁问题。这个库是认真考虑过 SQLite 并发现实的。

### 12.4 认证和业务信息分层清楚

它没有把订阅、profile、组织等业务概念强塞进 auth 内核，只暴露了 `GetInfo` 这种扩展点。

### 12.5 OAuth / SAML 都有

这意味着你可以先从 email/password 起步，后面再逐步加 Google、企业 SSO，而不必换整套 auth。

### 12.6 很符合“小而稳”的单体站点哲学

这基本就是作者几个项目共同体现出来的方法论。

## 13. 这个库哪里需要我们小心

虽然我认为它值得采用，但也有几个点需要看清楚。

### 13.1 API 风格偏传统

如果你的前端是现代 SPA / app shell 风格，那你大概率不想直接把 `/user/*` 原样暴露给前端。

更稳妥的做法是：

- 内部继续用 `smhanov/auth`
- 外层封装一组自己的 `/auth/*` JSON API

### 13.2 路由和字段命名是作者个人风格

例如：

- `/user/get`
- `/user/auth`
- `/user/create`

它们并不是现代 REST 或 JSON API 里最常见的命名。

这不是什么大问题，但说明“直接原样暴露给前端”会让你的业务接口风格有点割裂。

### 13.3 rate limit 是单进程的

单机没问题，多实例就不够了。

### 13.4 OAuth 缺邮箱时使用占位 email

这是一种很实用的工程妥协，但如果你以后对邮箱身份、通知投递、用户唯一性有更严格要求，就需要在业务侧额外约束。

### 13.5 它不是 headless identity platform

它更像一个“完整认证组件”，不是 Clerk/Auth0/Supabase Auth 那种产品。

也就是说：

- 它适合嵌进你的单体系统
- 不适合期待它成为独立 IAM 平台

## 14. 我们该怎么处理它

结合我们前面的讨论，我认为现在结论更稳了：

- 正式主方案应当是 `smhanov/auth`
- 我们本地之前的 `boringauth` 不应该恢复
- 未来应该做的是“基于它接入”或“基于它包一层”

最合理的落地姿势是：

### 方案 A：直接接入，最快

适合：

- 后端渲染页面
- 传统网站
- 想尽快上线

做法：

- 直接挂 `/user/`
- 页面表单直接对接它

### 方案 B：外包一层 JSON adapter，最推荐

适合：

- 你有前后端分层
- 想统一 API 风格
- 想保留服务端 session 模型

做法：

- 内部保留 `smhanov/auth` 作为内核
- 对外暴露自己的 `/auth/login`、`/auth/me`、`/auth/logout` 等接口
- 由适配层调用内部 handler / service

### 方案 C：fork 后有限改造

适合：

- 你要长期维护
- 你确定会反复复用
- 你希望路由、返回结构、hooks 更符合你的偏好

但即便 fork，也建议有限改造，不要推翻它的：

- session 模型
- SQLite / Postgres schema 基本结构
- password reset 主流程
- OAuth / SAML 主流程

## 15. 结合作者几个网站后的最终理解

在代码层面看完 `smhanov/auth` 之后，我对作者几个项目里的认证实现有了更清晰的判断：

- 他不是每个项目各写一套登录
- 而是把认证做成一个稳定内核
- 项目侧只接入、配置和补业务信息

这和他整体的工程风格完全一致：

- 把重复出现、但不想反复重写的“脏活累活”抽成可复用内核
- 产品项目本身尽量保持单体、低成本、少折腾

也因此，`smhanov/auth` 对我们最有价值的地方，不只是“省代码”，而是它提供了一条很清楚的工程路线：

- 对独立开发者来说，认证不需要从零自研
- 也不一定要接第三方 SaaS
- 完全可以用一个自托管、传统、稳定的 Go 内核解决

## 16. 当前结论

最终结论如下：

1. `smhanov/auth` 是一个值得采用的正式方案，不是 demo。
2. 它最适合单体 Go 网站、自托管、SQLite/Postgres、服务端 session 这一路。
3. 如果我们要用，最推荐的方式不是重写 auth，而是包一层我们自己的 JSON API。
4. 从作者代码风格和项目风格看，这就是他真正长期复用的一套认证底座。

## 17. 后续可做事项

如果后续继续推进，最值得做的不是再写一份 auth 设计文档，而是下面两件事之一：

1. 写一份“如何把 `smhanov/auth` 接入现有项目”的落地文档。
2. 直接实现一层薄的 `/auth/*` JSON 适配层设计，明确前后端交互方式。

