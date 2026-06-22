# Progress: Public Deploy Auth — 2026-06-22 完成

## 当前状态

**分支:** `feat/public-deploy-auth`(已推送 `origin/feat/public-deploy-auth`,本地与远端一致)
**HEAD:** `43c85ae test: e2e-auth.sh — cookie auth + history + usage limit smoke`
**工作树:** 干净

**整体进度:** 7 Phase / 23 Task **全部完成 ✅**(Phase 1–7 全绿)。上一暂停点见 [2026-06-21 笔记](2026-06-21-public-deploy-auth-progress.md)(Phase 1–5,16 个 Task)。

## 本次完成(Phase 6 + 7,7 个 Task)

| Phase | Task | Commit | 说明 |
|---|---|---|---|
| 6.1 | 落地页 `/` | `4202d54` | `LLM_WIKI_PUBLIC_LANDING_DIR` 开关 + `serve_file` 辅助 + `index/landing.css/landing.js`。开关默认关,本地开发不变 |
| 6.2 | 登录/注册页 | `9b3c1de` | `/login`+`/register` 同页 tab 切换 + `auth/{login.html,auth.css,auth.js}`。收窄 `is_auth` 让 `GET /auth/*.{css,js}` 落到静态分发(详见下方"关键设计点") |
| 6.3 | 重置密码页 | `fb00476` | `/reset-password` 两步页(忘记→取 stderr token→重置)。复用 6.2 的 auth.css |
| 6.4 | lite 移动端 | `42a44cd` | off-canvas 抽屉侧边栏(768px 断点)+ 汉堡菜单显隐。纯 CSS(DOM/handler 5.3 已就位) |
| 7.1 | 部署文档 | `2f09497` | `docs/部署-ECS与Tunnel.md` §3.6.1 公网模式 runbook + `README-OVERLAY.md` 指针 |
| 7.2 | 回归 + e2e | `43c85ae` | `scripts/e2e-auth.sh` cookie 闭环冒烟脚本 |

(6.1 在本次会话开始时落地,其 code-review 细节见上一笔记。)

## 关键设计点:6.2 的 auth 静态资产路由

6.1 review 时发现一个隐患:`login.html` 引用 `/auth/auth.css` 与 `/auth/auth.js`,但 `dispatch_request` 把所有 `/auth/*` 路由到 auth API(未知路径返回 404 JSON),静态 CSS/JS 会 404。6.1 当时**移除了**那个死代码分支(不可达,因 `is_auth` 先拦截)并标注推迟到 6.2 解决。

6.2 的解法(在 `server.rs` `dispatch_request`):
```rust
// Auth static assets (GET /auth/*.css|js) are served from the public
// landing dir, not the auth API. Exclude them so they fall through to
// the landing branch below; everything else under /auth/ is the API.
let is_auth_asset = method == Method::Get
    && path.starts_with("/auth/")
    && (path.ends_with(".css") || path.ends_with(".js"));
let is_auth = path.starts_with("/auth/") && !is_auth_asset;
```
然后在 landing 分支重新加回 match arm:
```rust
other if other.starts_with("/auth/")
    && (other.ends_with(".css") || other.ends_with(".js")) =>
{
    Some(&other[1..])  // strip leading "/", e.g. "auth/auth.css"
}
```
要点:**只有 GET 的 .css/.js 落静态**;POST `/auth/auth.css` 仍走 API → 404(静态文件不写)。冒烟确认:`GET /auth/auth.css`=200、`POST /auth/auth.css`=404。

## 6.4 的 plan 偏离(基于真实 DOM)

plan §6.4 假设要新建 `#menu-toggle` 汉堡和 `.sidebar` 类,但 Task 5.3 的真实 DOM 已有:
- `#btn-menu`(类 `icon-btn`,默认 `hidden`)—— index.html:13
- `#history-sidebar`(类 `history-sidebar`)—— index.html:45
- toggle handler 已在 app.js:616:`$("#btn-menu")?.addEventListener("click", () => { $("#history-sidebar")?.classList.toggle("open"); });`

因此 6.4 **只改了 `app.css`**:768px 断点下让 `.history-sidebar` 变 fixed 抽屉(`transform: translateX(-100%)` 默认隐藏,`.open` 时滑入),并用 `#btn-menu[hidden] { display: inline-flex; }` 在移动端显汉堡。无需改 html/js。

## 验证结果(本次)

- **cargo test** server crate:1 单元测试通过
- **cargo test** auth crate:**30 个集成测试全绿**(register/login/session/forgot/reset/usage/timing 等)
- **6.2 冒烟**:`/login`=200、`/auth/auth.css`=200、`/auth/auth.js`=200、`/auth/me` 无 cookie=401、register→me(cookie)→logout→login 全闭环
- **6.3 冒烟**:完整 reset 流程——forgot→stderr 取 token→reset=ok→新密码登录 ok、旧密码 401、token 复用 400(单次使用)
- **6.4**:CSS brace 平衡(117/117),handler 已就位
- **7.2 `e2e-auth.sh`**:landing/register/me(limit=3)/conversations CRUD/logout→401/bearer 全 ok。chat 用量限额步因本机无真实 wiki 语料返回 404(脚本已 best-effort + 说明,符合预期;真正验 chat 429 需在部署机带真实 project 跑)

## 本机环境限制(给下次接手)

- **本机原先无 Rust 工具链**:本次用 `rustup` 装了 stable 1.96.0(`~/.cargo`),`cargo` 需 `. "$HOME/.cargo/env"` 后才在 PATH
- **`upstream` 子模块未初始化**:`upstream/dist` 不存在,故 6.4 的 dist sync 与 `e2e-local.sh` 无法在此机跑;二者属部署时 `build-web.sh` 生成 / 需真实 wiki 语料
- plan 里的项目路径(`/home/li/overseas-github/.../CivilCareer`)与 config(`server.minimax.local.json`)在本机不存在,冒烟用 `LLM_WIKI_PROJECT=$PWD` + 无 config 跑通(landing/auth 路径不依赖 wiki 内容)

## ⚠️ 工作流提醒(沿用上一笔记)

每次 amend/commit 后务必 `git status` 确认工作树干净,且 `git show HEAD:<file>` 与工作区一致——尤其前端文件。本次每个 Task 后都核对了。

子代理派发偶尔因 `glm-5.2 is temporarily unavailable` 分类器暂时不可用而失败;`pkill` 等命令也会被拦。遇到就 `sleep` 后重试,或对纯前端/简单 Task 自己直接改。

## 留下的已知设计权衡(不阻塞 v1,沿用上一笔记)

- `RateLimiter` HashMap 无界增长;chat 用量 get_usage+increment 有 TOCTOU 窗口
- `start_password_reset` / `register` 未限流
- `From<rusqlite::Error>` 用字符串匹配 UNIQUE(脆弱,v1.1 改 extended_code 2067)
- 重置 token 只打 stderr(无真实邮件发送)——部署文档 §3.6.1 已说明用 `journalctl` 取
- 7.2 的 `e2e-auth.sh` chat 用量限额步需真实 project 才能验到 200×3 + 429(本机只能验到 404)

## 下一步建议

整支 feature 已 23/23 全绿,可考虑:
1. 合并到 `main`(或开 PR 走 review)
2. 在部署机(有真实 wiki 语料 + built UI)跑 `e2e-local.sh` + `e2e-auth.sh` 完整回归(含 chat 429)
3. v1.1:邮件发送、限流加固、`From<rusqlite>` 用 extended_code(见 plan §已知遗留)

## 接手三步

1. `git checkout feat/public-deploy-auth && git pull`
2. 读这个文件
3. 如需部署:按 `docs/部署-ECS与Tunnel.md` §3.6.1 配 env,在部署机跑 `e2e-auth.sh`
