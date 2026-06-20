# Progress: Public Deploy Auth — 2026-06-20 暂停点

## 当前状态

**分支:** `feat/public-deploy-auth`(已推送 `origin/feat/public-deploy-auth`)
**HEAD:** `5f257c1 chore(auth): commit Cargo.lock and ignore overlay/auth/target`
**工作树:** 干净(除 `m upstream` 子模块工作区脏内容,与本次工作无关)

**整体:** 7 Phase / 23 Task 中,**完成 10 个 Task(Phase 1–2 全部 + Phase 3 Task 3.1)**;还剩 13 个。

## Plan 与 Spec

- Spec: `docs/superpowers/specs/2026-06-20-public-deploy-auth-design.md`
- Plan: `docs/superpowers/plans/2026-06-20-public-deploy-auth.md`
- 参考: `docs/低成本网站经验/`(smhanov/auth 等)
- 已 clone 的参考代码(可重新 clone): `git clone https://github.com/smhanov/auth /tmp/smhanov-auth`

## 已完成的 Task(共 10)

| # | Task | Commit | 说明 |
|---|---|---|---|
| 1.1 | crate 骨架 | `b955e5f` | `overlay/auth/` 新 crate,server 依赖,空模块 |
| 1.2 | SQLite schema + init_schema | `de73bb6` | 6 张表 + 3 索引 + 4 pragmas |
| 1.3 | argon2id 密码 | `e72b49c` | hash + verify + PasswordError |
| 1.4 | session token + cookie | `f5fe327` | 32B base64url + sha256 + Set-Cookie 构造 |
| 2.1 | AuthError | `799a04c` | 9 变体 + code/http_status/user_message + From<rusqlite/PasswordError>。**已 amend**:加入了 SQLite 错误字符串匹配脆弱性的注释(指向 v1.1 用 extended_code 2067 的更稳路径) |
| 2.2 | Store(rusqlite) | `11fc6aa` | users/sessions/reset_tokens/usage 全套 CRUD,Mutex<Connection> |
| 2.3 | 漏桶限流 | `f5603bf` | RateLimiter + allow(),时间参数化 |
| 2.4 | AuthService | `d4af128`(amend 自 `0f3ecb8`) | register/login/logout/session_user。**已 amend**:加 `dummy_hash` + 在 unknown-email 路径执行一次 dummy verify_password,关闭 timing oracle |
| 2.5 | forgot/reset 密码 | `025e7a9` | 单次使用 + 1h TTL + 重置后清空所有 session |
| 3.1 | server 配置项扩展 | `3b0f667` | `auth_db / require_login / daily_chat_limit / admin_email / session_ttl_days` 加到 ServerConfig + Args + resolve() |
| (杂项) | Cargo.lock + .gitignore | `5f257c1` | 把累积的 lock/gitignore 整理一起提交 |

测试:`cargo test -p llm-wiki-auth` 共 **29 个集成测试全绿**(schema 2 + password 2 + session 4 + store 6 + ratelimit 3 + service 7 + reset 5)。

## 留下的已知设计权衡(不阻塞 v1)

代码 reviewer 提到几条真实但不阻塞的项,plan 已显式不在 v1 内:

- 内存 `RateLimiter` 的 HashMap 无界增长(smhanov 有 10min 周期清理,我们没做)
- read-then-delete reset token 在 `Mutex<Connection>` 下安全,但跨方法不是单事务——超低概率竞态
- `start_password_reset` 没有限流(未注册邮箱与已注册邮箱时序仍略有差异;HTTP 响应已统一)
- `register` 没有限流(设计上交给 `LLM_WIKI_REQUIRE_INVITE` 这类未来手段)
- `resolve()` 现在是 10 个位置参数,顺序错调换不会被编译器抓——沿现有风格未重构

这些都登记在 plan 的 "Self-Review / 已知遗留" 部分。

## 下一个要做的 Task: 3.2

**ServerState 持有 AuthService**(plan 文件 §Task 3.2)。要做的事:

1. `overlay/server/src/state.rs`:`ServerStateInner` 加 `auth: Option<Arc<AuthService>>`、`require_login: bool`、`daily_chat_limit: u32` 三字段;加 `with_auth(self, auth, require_login, daily_chat_limit) -> Self` 链式注入(不 mutate,重建 inner Arc);加 `auth() / require_login() / daily_chat_limit()` getter
2. `overlay/server/src/main.rs`:在 `ServerConfig::resolve` 之后,如果 `config.auth_db` 是 Some,调用 `Store::open` + `AuthService::new`(`session_ttl_secs = days * 86400`,`login_attempts=25.0`,`login_period_secs=3600.0`),失败 fail-fast 退出
3. `overlay/server/src/server.rs`:`run` 签名改为 `run(config, auth: Option<Arc<AuthService>>)`,内部 `ServerState::from_config(&config).with_auth(auth, ...)`
4. 编译 + 冒烟两种模式(无 `LLM_WIKI_AUTH_DB` 仍 OK;有 `LLM_WIKI_AUTH_DB` 自动建 SQLite 文件)
5. Commit 信息:`feat(server): construct AuthService at startup, attach to ServerState`

派发 implementer 子代理的完整 prompt 已在 plan 的 Task 3.2 里;直接读 plan 复制即可。

## 接下来剩余 Task(供新会话总览)

- **Phase 3 剩余:** 3.3 `/auth/*` HTTP 路由
- **Phase 4:** 4.1 cookie-aware `authorize()` · 4.2 chat 用量计数
- **Phase 5:** 5.1 Store 扩 conversations/messages · 5.2 `/api/v1/conversations*` · 5.3 lite 页改造
- **Phase 6:** 6.1 落地页 · 6.2 登录/注册页 · 6.3 重置密码页 · 6.4 lite 移动端
- **Phase 7:** 7.1 部署文档更新 · 7.2 e2e-auth.sh 回归脚本

## 流程提醒(给下次接手的我自己)

执行方式是 **superpowers:subagent-driven-development**:每个 Task 派一个新鲜的 implementer 子代理 → spec compliance reviewer → code quality reviewer → 通过后标 done 跳下一个。Reviewer 模板在 plan skill 里,prompt 也在 plan 文件里复制即可。

TaskList 里的 23 条 todo 已是最新;Task IDs 19 已完成,20 是 Task 3.2,从 20 开始 in_progress。

明天开新会话:第一步读这个文件(`docs/notes/2026-06-20-public-deploy-auth-progress.md`),第二步 `git checkout feat/public-deploy-auth`,第三步从 plan §Task 3.2 派发 implementer。
