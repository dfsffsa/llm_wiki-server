# Progress: Public Deploy Auth — 2026-06-21 暂停点

## 当前状态

**分支:** `feat/public-deploy-auth`(已推送 `origin/feat/public-deploy-auth`,本地与远端一致)
**HEAD:** `d100b4b feat(lite): cookie-auth gate, server-side history, user bar with quota`
**工作树:** 干净(除 `m upstream` 子模块工作区脏内容 —— 是 dist 同步产生的,属正常,部署时 `build-web.sh` 重新生成)

**整体进度:** 7 Phase / 23 Task 中,**完成 16 个 Task(Phase 1–5 全部)**;还剩 7 个(Phase 6 + 7)。

## Plan 与 Spec(不变)

- Spec: `docs/superpowers/specs/2026-06-20-public-deploy-auth-design.md`
- Plan: `docs/superpowers/plans/2026-06-20-public-deploy-auth.md`(每个 Task 的完整代码 + 步骤都在里面,直接复制即可派发)
- 参考: `docs/低成本网站经验/`;smhanov/auth 可 `git clone https://github.com/smhanov/auth /tmp/smhanov-auth`

## 已完成的 Task(共 16,Phase 1–5 全绿)

| Phase | Task | Commit | 说明 |
|---|---|---|---|
| 1.1 | crate 骨架 | `b955e5f` | `overlay/auth/` 新 crate |
| 1.2 | SQLite schema | `de73bb6` | 6 表 + 3 索引 + 4 pragmas |
| 1.3 | argon2id 密码 | `e72b49c` | hash + verify |
| 1.4 | session token + cookie | `f5fe327` | 32B base64url + sha256 + Set-Cookie |
| 2.1 | AuthError | `799a04c` | 9 变体 + From<rusqlite>(含脆弱性注释) |
| 2.2 | Store | `11fc6aa` | users/sessions/reset/usage CRUD |
| 2.3 | 漏桶限流 | `f5603bf` | RateLimiter |
| 2.4 | AuthService | `d4af128` | register/login/logout/me(含 timing-oracle 防御 dummy_hash) |
| 2.5 | forgot/reset | `025e7a9` | 单次使用 + 1h TTL + 清 session |
| 3.1 | server 配置项 | `3b0f667` | auth_db/require_login/daily_chat_limit/admin_email/session_ttl_days |
| 3.2 | ServerState 持 AuthService | `085ded0` | with_auth + 启动构造 |
| 3.3 | /auth/* 路由 | `9043365` | register/login/logout/me/forgot/reset |
| 4.1 | authorize() cookie | `a21c189` | AuthOutcome 枚举,cookie→bearer→anonymous |
| 4.2 | chat 用量限额 | `401043b` | Cookie 用户每日限额,Bearer 绕过(含 TOCTOU 注释) |
| 5.1 | Store conversations | `c29548e` | 7 方法 + 2 row struct |
| 5.2 | /api/v1/conversations* | `71238be` | 5 路由,cookie-only,所有权隔离 404(含 CORS DELETE 修复) |
| 5.3 | lite 页改造 | `d100b4b` | 登录拦截 + 服务端历史 + 用户栏/额度 + 3 个 review 修复 |

**测试:** `cargo test --manifest-path overlay/auth/Cargo.toml` → **30 个集成测试全绿**。
**构建:** `cargo build --release --manifest-path overlay/server/Cargo.toml` → 成功(仅 3 个预期的 dead_code warning:`AuthOutcome::user_id()`、`require_login`、`invalidate_config_cache`,均待后续 Task 消费或属预存)。

## Task 5.3 的 3 个 review 修复(已并入 `d100b4b`)

代码 reviewer 发现并已修:
1. **Critical — 共享 token 模式不坏**:`/auth/me` 无 auth_db 时返回 500,旧逻辑会让 lite 无条件跳 `/login` → 本地开发坏。修法:`fetchMe` 返回 `{status: ok|no-auth|disabled}`,仅 401 才跳转,500/网络错误判 `disabled` 继续用 Bearer。`renderTopbar` 在无 user 时隐藏顶栏。
2. **Important — dist sync**:`upstream/dist/lite/index.html` 缺 `id="history-sidebar"`,移动端 toggle 失效。重新 `cp` 三文件,diff 确认 byte-identical。
3. **Minor — 429 显示原始 JSON**:streamChat 错误路径改先 `JSON.parse` 取 `error.message`,catch 的配额检测拓宽到 `/daily_limit|额度|429/i`。

**两种模式冒烟均过**(共享 token 模式 lite+Bearer 正常;auth 模式 401/200/conversations 正常)。

## ⚠️ 一个重要的工作流提醒(给下次接手)

Task 5.3 期间出现过 **HEAD 与工作区分叉**:amend 时提交的 app.js 是旧版本,工作区是含修复的新版本。我已对齐(把工作区版本 amend 进 HEAD,工作树现干净)。**教训:每次 amend/commit 后,务必 `git status` 确认工作树干净,且 `git show HEAD:<file>` 与工作区文件一致**,尤其前端文件容易被 linter/子代理重写。下次接手第一件事:`git status` + `git log --oneline -3` + 抽查关键文件。

## 留下的已知设计权衡(不阻塞 v1,reviewer 都批准了)

- `RateLimiter` HashMap 无界增长(smhanov 有 10min 清理,我们没做)
- chat 用量 get_usage+increment 有 TOCTOU 窗口(并发可超限 N-1,已加注释)
- `start_password_reset` 没限流;`register` 没限流
- `require_login` 标志存在但 `authorize()` 没强制执行(当前 minimax 配置 allowUnauthenticated=false,anonymous 不会出现,无影响;真正需要时在 authorize 或 chat 处补)
- `From<rusqlite::Error>` 用字符串匹配 UNIQUE(脆弱,注释里给了 v1.1 的 extended_code 2067 方案)
- 多处 `now_secs()`/`today_utc()` 重复(4-5 份,DRY 债,Phase 7 可清)
- error 响应两种 shape(`{ok:false,error}` vs `{error:{code}}`),前端已适配

## 下一个要做的 Task: 6.1 落地页

**还没开始**(todo 已回退到 pending)。Plan 文件 §Task 6.1 有完整步骤。要点:

1. **配置项**:ServerConfig 加 `public_landing_dir: Option<PathBuf>`;main.rs Args 加 `LLM_WIKI_PUBLIC_LANDING_DIR`
2. **server.rs 托管**:`dispatch_request` 在 static_files 分支前,若 `public_landing_dir` 配置了,把 `/`→`index.html`、`/login`/`/register`→`auth/login.html`、`/reset-password`→`auth/reset.html`、`/landing.css`/`/landing.js`/`/auth/*.{css,js}` 从该目录托管(优先于 upstream/dist)
3. **静态文件**:写 `overlay/static/index.html`(落地页)、`landing.css`、`landing.js`(调 `/auth/me` 决定按钮跳 `/lite/` 或 `/login`)
4. 需要 `static_files.rs` 加一个 `serve_file(root, rel)` 辅助(现有 `serve_static(root, &path)` 是按完整 path 托管)
5. 冒烟:`LLM_WIKI_PUBLIC_LANDING_DIR=$PWD/overlay/static` 启动,`curl /` 见"开始使用"
6. Commit:`feat(landing): public landing page (overlay/static), gated by LLM_WIKI_PUBLIC_LANDING_DIR`

**注意:** Task 6.1 涉及 Rust(server.rs/config.rs/static_files.rs)+ 前端(3 个静态文件),派 implementer 子代理时把 plan §Task 6.1 的代码整段给它。落地页让 `/` 显示极简页而非重 React UI——通过 `LLM_WIKI_PUBLIC_LANDING_DIR` 开关,默认关(本地开发不变)。

## 剩余 Task 总览(7 个)

- **Phase 6:** 6.1 落地页 · 6.2 登录/注册页(`/login`/`/register` 映射到 `auth/login.html`,tab 切换) · 6.3 重置密码页 · 6.4 lite 移动端适配(`@media` + 汉堡菜单,`#btn-menu` 已在 DOM 里,`#history-sidebar` id 已就位)
- **Phase 7:** 7.1 部署文档更新(`docs/部署-ECS与Tunnel.md` 加 §3.6.1 公网模式) · 7.2 e2e-auth.sh 回归脚本

## 流程提醒(给下次接手的我自己)

执行方式 **superpowers:subagent-driven-development**:每个 Task 派 implementer 子代理 → spec compliance reviewer → code quality reviewer → 通过标 done。Reviewer/reviewer prompt 模板在 skill 里,implementer prompt 直接从 plan 文件复制 Task 全文。

**注意:** 子代理派发偶尔会因模型分类器暂时不可用而失败(`glm-5.2 is temporarily unavailable`)。如果遇到,稍等重试;或对纯前端/简单 Task 自己直接改(像 Task 5.3 的 review 修复我就是自己改的)。

TaskList 里 23 条 todo 是最新状态:1–26 completed,27(6.1)pending,28–32 pending。从 27 开始。

## 接手三步

1. `git checkout feat/public-deploy-auth && git pull`
2. 读这个文件(`docs/notes/2026-06-21-public-deploy-auth-progress.md`)
3. 从 plan §Task 6.1 派发 implementer(或自己写)
