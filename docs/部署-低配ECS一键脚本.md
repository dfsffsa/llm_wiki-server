# 低配 ECS 一键部署 runbook

> 适用：目标服务器 ≤ 2GB RAM（典型 1.6GB / 1–2 vCPU），**无法**本地 `cargo build`（lancedb 链接阶段必 OOM）。
> 思路：本地交叉编译 musl 静态二进制 + Vite 预构建 + Node 依赖，整体 rsync 上传，systemd 拉起。
> 相关：[低配机交叉编译CLI.md](./低配机交叉编译CLI.md) · [部署-ECS与Tunnel.md](./部署-ECS与Tunnel.md)

---

## 1. 目标机器硬约束

| 资源 | 经验值 | 说明 |
|------|--------|------|
| RAM | 1.6 GB | LanceDB + Arrow + DataFusion 链接时峰值 ~1.5 GB，临界 OOM；server 单独 2GB 内存能编但慢 |
| 磁盘 | 10 GB+ | 远端需 `upstream/node_modules` 482 MB + `overlay/cli/node/node_modules` ~150 MB + 二进制 + 数据 |
| 网络 | 出站可访问 crates.io 或国内镜像 | 直连编译时下载 stage 反复超时 |
| OS | Ubuntu 22.04 / Debian 12 | musl 静态二进制可跑到任意 x86_64 Linux |

任何一项不满足，先解决再上脚本，否则部署上去也会在 Node 依赖安装阶段失败。

---

## 2. 与通用 ECS 部署的差异

[部署-ECS与Tunnel.md](./部署-ECS与Tunnel.md) 假设的是 2C4G + root + 完整 rsync 流程。低配变体有几个不同点：

| 项目 | 通用 ECS | 低配变体 |
|------|----------|----------|
| 构建 | 可在 ECS 上 `cargo build` | **必须**本地交叉编译 musl 静态 |
| systemd User | `deploy` | `root`（小机无 sudo 配置时间） |
| 代码路径 | `/opt/llm-wiki/` | `/root/llm_wiki-server/` |
| Wiki 路径 | `/data/wiki/<项目>` | `/root/llm_wiki_projects/<项目>` |
| Node 依赖 | `overlay/cli/node` 装上即可 | 若 ECS 跑 ingest：**还要**装 `upstream/node_modules`（ingest 子进程要 zustand/milkdown）；只读+chat 无需 Node |
| 部署方式 | 手工 rsync + systemctl | `./scripts/deploy-ecs.sh` 一键 |
| 端口 | 8080 | 视占用情况（8080/8081/…），先 `ss -ltnp` 查 |

---

## 3. SSH 连通性（前置）

阿里云安全组常**只开放 22022**（不开放 22）。先确认目标端口：

```bash
# 22 不通
ssh -o ConnectTimeout=5 user@47.103.39.152 'echo OK'   # timeout

# 22022 通
ssh -p 22022 user@47.103.39.152 'echo OK'
```

后续部署脚本用 `SSH_PORT=22022` 即可，不要为这一台机器改全局 `~/.ssh/config`。

如果**整套机器都从同一台跳板/办公机连**（如 wanghuacun），把 `Host` 别名和 `Port` 写在专门的 ssh config 里更清爽：

```ssh-config
# ~/.ssh/config.d/ecs.conf
Host llm-wiki-ecs
    HostName 47.103.39.152
    Port 22022
    User root
    ServerAliveInterval 30
    ServerAliveCountMax 3
    StrictHostKeyChecking accept-new
```

部署时 `SSH_HOST=llm-wiki-ecs SSH_CONFIG=~/.ssh/config.d/ecs.conf ./scripts/deploy-ecs.sh` 即可。

---

## 4. 本地一次构建

按 [低配机交叉编译CLI.md §3–§6](./低配机交叉编译CLI.md) 准备环境后：

```bash
# 1) server（~2 min 增量，5 min 全量）
cargo build --release \
  --manifest-path overlay/server/Cargo.toml \
  --target x86_64-unknown-linux-musl

# 2) CLI（首次 5–15 min，增量 1–3 min；含 lancedb 重）
cargo build --release \
  --manifest-path overlay/cli/rust/Cargo.toml \
  --target x86_64-unknown-linux-musl

# 3) UI（必须用 http 模式 + 同源 token）
export VITE_API_TOKEN='minmax2.7'   # 与 LLM_WIKI_API_TOKEN 一致
VITE_BACKEND=http VITE_API_TOKEN="$VITE_API_TOKEN" ./scripts/build-web.sh
```

验证产物：

```bash
file overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server
# 期望: ELF 64-bit LSB pie executable, ..., static-pie linked, stripped

file overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki
ldd overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server
# 期望: ldd 报 "not a dynamic executable"
```

如果 `file` 输出 `dynamically linked`、或 `ldd` 列出 `linux-vdso.so.1` 之外的依赖：说明没编成 musl 静态，重新检查 `.cargo/config.toml` 的 `linker = "musl-gcc"`。

---

## 5. 一键部署

### 5.1 准备 server 配置

```bash
# 复制样例
cp overlay/config/server.example.json overlay/config/server.local.json

# 编辑：projects[].path 指向 /root/llm_wiki_projects/<项目>
#       llmConfig.apiKey 改成 PLACEHOLDER_FILL_ON_SERVER（脚本会替换）
#       llmConfig.model / customEndpoint 按你的 LLM 提供商填写
```

> `server.local.json` 已被 `.gitignore`（`*.local.json`）忽略，不会进 git。

### 5.2 跑部署脚本

```bash
export LLM_API_KEY='sk-真实的key'   # 真实密钥，脚本会注入 server.local.json
SSH_HOST=root@47.103.39.152 \
SSH_PORT=22022 \
SERVER_PORT=8081 \
LLM_API_KEY="$LLM_API_KEY" \
  ./scripts/deploy-ecs.sh
```

脚本一次完成：

1. 校验本机构建产物
2. 测试 SSH
3. 准备远端目录
4. 上传 server / CLI / `upstream/dist` / `upstream/src`（增量）
5. 注入真实 `LLM_API_KEY` 到 `server.local.json`，chmod 600
6. 远端 `npm ci overlay/cli/node`（含 tsx；仅 ingest 需要）
7. 远端 `npm ci upstream/`（ingest 子进程需要 zustand/milkdown 等；只读+chat 可跳过）
8. 写 systemd unit
9. 启动服务
10. 校验 `/api/v1/health` 与 `/api/v1/projects`

### 5.3 参数一览

| 变量 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `SSH_HOST` | ✅ | — | `user@ip` 或 ssh config 里的 Host 别名 |
| `LLM_API_KEY` | ✅ | — | 真实 LLM 密钥；不会进 git |
| `SSH_PORT` | | 22 | SSH 端口（阿里云常 22022） |
| `SSH_CONFIG` | | 空 | ssh config 文件路径（用别名时设） |
| `SERVER_REPO` | | `/root/llm_wiki-server` | 远端代码目录 |
| `SERVER_WIKI_ROOT` | | `/root/llm_wiki_projects` | 远端 wiki 根 |
| `SERVER_PORT` | | 8080 | 远端监听端口（先 `ss -ltnp` 查占用） |
| `SERVER_BIND` | | `127.0.0.1:${SERVER_PORT}` | 监听地址 |
| `SERVER_TOKEN` | | `minmax2.7` | API Bearer token；生产建议改强随机 |

### 5.4 部署完成检查

```bash
ssh -p 22022 root@47.103.39.152 '
  systemctl status llm-wiki-server --no-pager | head -15
  echo "---"
  curl -sS -H "Authorization: Bearer minmax2.7" \
    http://127.0.0.1:8081/api/v1/health
  echo
  curl -sS -H "Authorization: Bearer minmax2.7" \
    http://127.0.0.1:8081/api/v1/projects | head -c 400
  echo
'
```

期望：

- `active (running)` + `enabled`
- `/api/v1/health` → `{"ok":true,...}`
- `/api/v1/projects` → 列出配置的多个项目

---

## 6. 日常增量更新

### 6.1 改 wiki/（rsync）

```bash
rsync -avz --progress --delete \
  ~/overseas-github/llm_wiki_projects/ParentingBooks/ \
  -e "ssh -p 22022" root@47.103.39.152:/root/llm_wiki_projects/ParentingBooks/
```

不需要重启 server（只读读盘）。

### 6.2 改代码 / 重新构建

**关键原则**：远端**不需要 `git clone` / `git pull`**。`deploy-ecs.sh` 和 `sync-artifacts.sh` 把所有运行时需要的文件都 rsync 过去了——二进制、`upstream/dist/`、`upstream/src/`（让 chat/ingest 子进程能解析 `@/` 别名）、`node_modules/`、config。远端只要 Node + systemd，**不需要 git 仓库、Rust 工具链、npm、protoc**。

| 远端需要 | 远端不需要 |
|----------|-----------|
| Node.js 20+（tsx chat/ingest 子进程） | git（不 `git clone` / `git pull`） |
| systemd（服务管理） | Rust 工具链 / cargo |
| `/root/llm_wiki-server/` rsync 目标目录 | npm / npx（node_modules 从 dev 机 rsync） |
| LLM API 可达（chat + ingest 用） | protoc / lancedb 编译依赖 |

**常规迭代**（代码或 UI 改了，配置和 systemd 不动）—— 用 `sync-artifacts.sh`，比 `deploy-ecs.sh` 轻得多：

```bash
# 本地：重新构建
./scripts/build-cli.sh   # cargo build musl，会自动复用 protoc
VITE_BACKEND=http VITE_API_TOKEN="$VITE_API_TOKEN" ./scripts/build-web.sh

# 本地：增量同步产物（首次 ~500MB；之后 rsync delta ~10s）
SSH_HOST=root@47.103.39.152 SSH_PORT=22022 ./scripts/sync-artifacts.sh

# 远端：重启服务（仅二进制或 dist 变化时需要）
ssh -p 22022 root@47.103.39.152 'systemctl restart llm-wiki-server'
```

`sync-artifacts.sh` 只 rsync 二进制 + dist + node_modules，不碰 systemd / config / wiki 数据，**常规迭代用这个**。

**全量首部署 / 换机器 / 改 systemd** —— 仍用 `deploy-ecs.sh`：

```bash
# 本地：重新构建（参考 §4）—— 增量通常 1–5 min
./scripts/build-cli.sh   # 等价于 cargo build musl，会自动复用 protoc
VITE_BACKEND=http VITE_API_TOKEN="$VITE_API_TOKEN" ./scripts/build-web.sh

# 部署（脚本会增量上传）
SSH_HOST=root@47.103.39.152 SSH_PORT=22022 SERVER_PORT=8081 \
  LLM_API_KEY="$LLM_API_KEY" ./scripts/deploy-ecs.sh
```

如果只改了 `upstream/src/`，脚本只会重传那部分（`rsync` 算法）。`upstream/dist/` 用 `--delete` 同步（旧的 chunked 文件会清掉）。

### 6.3 改 wiki 路径 / 新增项目

编辑 `overlay/config/server.local.json`（在本地），rsync 到远端覆盖：

```bash
rsync -avz overlay/config/server.local.json \
  -e "ssh -p 22022" root@47.103.39.152:/root/llm_wiki-server/overlay/config/
ssh -p 22022 root@47.103.39.152 'systemctl restart llm-wiki-server'
```

`server.local.json` 在远端是 `chmod 600`，**不能**用 `sudo` 覆盖；用 root 登录即可。

---

## 7. 坑位清单（踩过的）

### 7.1 Vite alias 顺序：HTTP 模式静默退化为桌面

`vite.config.ts` 的 `resolve.alias` 是**前缀匹配、按数组顺序遍历**。`@/commands/fs`、`@/lib/llm-client` 等具体条目**必须**排在通用 `@` 之前，否则 vite 永远匹配 `@` 走 Tauri 路径，HTTP 模式下 `bootstrapHttpProject()` 返回 `null`，UI 自动 fall back 到桌面入口，看起来"能跑"但 search/chat 全走错路径。

**正确写法**（`upstream/vite.config.ts`）：

```ts
alias: isHttpBackend
  ? [
      { find: "@/commands/fs",          replacement: path.join(overlayWeb, "commands/fs.ts") },
      { find: "@/commands/file-sync",   replacement: path.join(overlayWeb, "commands/file-sync.ts") },
      { find: "@/lib/search",           replacement: path.join(overlayWeb, "lib/search.ts") },
      { find: "@/lib/project-store",    replacement: path.join(overlayWeb, "lib/project-store.ts") },
      { find: "@/lib/llm-client",       replacement: path.join(overlayWeb, "lib/llm-client.ts") },
      { find: "@/lib/persist",          replacement: path.join(overlayWeb, "lib/persist.ts") },
      { find: "@",                      replacement: path.resolve(__dirname, "./src") },
    ]
  : [{ find: "@", replacement: path.resolve(__dirname, "./src") }],
```

对象 spread 形式（`{ ...specific, "@": ... }`）看着对，但 JS 对象遍历顺序对 vite 不友好，**用数组**。这块对应 `overlay/patches/0002-http-ui-bootstrap.patch`，升级 upstream 时若 patch 冲突，重点看这里。

### 7.2 tsx 是 devDep 但 ingest 运行时必需

> **Chat 已不走 Node。** 自 reqwest 重写后，`/chat` 在 Rust server 进程内直连 LLM，不再 spawn `cmd-llm-stream.ts`。下面的 tsx / node_modules 要求现在**只针对在远端跑 `ingest` 的场景**；只部署只读+chat 服务则可忽略，远端甚至不需要 Node。

`overlay/cli/node` 的 ingest 子进程走 `node <tsx cli.mjs> …/cmd-ingest.ts`（直接 drive node + tsx CLI 模块，非 `npx`，避免 batch hang）。`tsx` 写在 `devDependencies` 里，但运行时 ingest 需要，所以**远端 `npm ci` 不能 `--omit=dev`**，否则 `Cannot find module 'tsx'`。

脚本里 `overlay/cli/node` 用的是 `npm ci`（默认装 dev），`upstream/node_modules` 才用 `--omit=dev`（体积大、生产不需要 React devtools 等）。不要统一加 `--omit=dev`。

### 7.3 ingest 子进程需要 upstream/node_modules

`cmd-ingest.ts` 通过 `overlay/cli/node/tsconfig.json` 的 `paths` 把 `@/lib/llm-client`、`@/lib/ingest` 等映射到 `upstream/src/`，而这些文件 `import { create } from "zustand"`、`@milkdown/...` 等。Node 解析器沿目录向上找 `node_modules`，所以 `upstream/` 下必须有 `node_modules`。

部署时只传 `overlay/cli/node/node_modules` 不够——必须也 `npm ci` `upstream/`。代价是远端约 482 MB 磁盘。**这是 ingest 的代价，不是 chat 的**——chat 已经是纯 Rust，零 Node 依赖。

### 7.4 端口冲突

`127.0.0.1:8080` 经常被占（searxng、nginx、其它 docker）。先 `ss -ltnp | grep 8080` 查清。脚本里 `SERVER_PORT=8081` 即可换端口，systemd 启动会用新端口。

### 7.5 22022 + ssh config 别名混用

`scripts/deploy-ecs.sh` 默认用 `-p 22`。如果目标机只开 22022：

- **不推荐**改全局 `~/.ssh/config`（影响其它机器）
- **推荐**两种写法选一：
  - `SSH_PORT=22022` 让脚本自己加 `-p 22022`
  - `SSH_CONFIG=~/.ssh/config.d/ecs.conf SSH_HOST=llm-wiki-ecs` 用别名（脚本自动加 `-F`）

不要两个都设导致 ssh 命令出现 `-F file -p 22022` 加 Host 别名，可能误用 file 里的 Port。

### 7.6 systemd 日志在 /var/log 不是 journalctl

脚本 unit 里显式 `StandardOutput=append:/var/log/llm-wiki-server.log`。优点：1.6GB 机器 systemd-journald 默认没开 persistent，append 到文件能稳定保留；缺点：没有 journalctl 的过滤便利，但 `tail -f`/`grep` 也够用。

### 7.7 rsync 增量 vs --delete

`upstream/dist/` 用 `--delete`：Vite build 会产出 hash 名文件，旧 chunk 不删会无限堆积。`upstream/src/` 不加 `--delete`：保留 git 历史痕迹方便排查，不占多少空间。

### 7.8 PLACEHOLDER_FILL_ON_SERVER 替换的安全性

`server.example.json` 里 `llmConfig.apiKey` 写 `PLACEHOLDER_FILL_ON_SERVER`（字面量字符串）。脚本 `sed` 替换后通过 `chmod 600` 保护。这个占位串**必须**和真实密钥**完全不同**且容易 `grep` 到，否则密钥可能以明文混进 `wiki/`（比如某次复用了同 prefix 的字符串作为示例）。

如果你的密钥前缀稳定（如 `sk-cp-...`），占位串可以用 `PLACEHOLDER_FILL_LLM_API_KEY_ON_DEPLOY` 这种更长的串，进一步降低误命中。

---

## 8. 故障排查速查

| 现象 | 原因 | 处理 |
|------|------|------|
| `error: linker 'musl-gcc' not found` | musl 工具链未装 | `apt install musl-tools` |
| `protoc out of date` | 系统 protoc < 3.21 | 见 [低配机交叉编译CLI §3.3](./低配机交叉编译CLI.md) |
| `file 二进制` 报 `dynamically linked` | `.cargo/config.toml` 未生效 | 检查 `linker = "musl-gcc"`、`--target` 漏写 |
| 远端 `Cannot find module 'tsx'` | `npm ci --omit=dev` 漏了 tsx | 脚本已 fix：cli/node 用 `npm ci`，不要改 |
| 远端 `Cannot find package 'zustand'` | 缺 `upstream/node_modules` | 跑 `ssh … 'cd upstream && npm ci --omit=dev'` |
| UI 打开后 search/chat 全 404 | HTTP bundle 没把 overlay adapter 打进去 | 检查 vite alias 顺序（§7.1） |
| `/api/v1/projects` 返回空 | `server.local.json` 中 path 不存在 | ssh 上去 `ls /root/llm_wiki_projects/` |
| Chat SSE 卡死 | `LLM_WIKI_REPO` 未设 / `upstream/src` 未传 | 脚本 unit 已设；检查 `/var/log/llm-wiki-server.log` |
| 401 Unauthorized | `VITE_API_TOKEN` 与 `LLM_WIKI_API_TOKEN` 不一致 | 重建 UI 或改 server token（需 rsync 重传 dist） |
| systemd `start request repeated too quickly` | 端口占用或配置错 | `journalctl -u llm-wiki-server -n 50` 看首次失败原因 |
| `ldd: not a dynamic executable` | 这是**期望输出** | musl 静态二进制正常表现 |

---

## 9. 公网暴露（后续）

低配 ECS 安全组一般不开 8080。公网 HTTPS 走 Cloudflare Tunnel，详见 [部署-ECS与Tunnel.md §3.8](./部署-ECS与Tunnel.md#38-cloudflare-tunnel)。简版：

```bash
# ECS 上
curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg \
  | sudo tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null
echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared bookworm main' \
  | sudo tee /etc/apt/sources.list.d/cloudflared.list
sudo apt update && sudo apt install -y cloudflared
cloudflared tunnel login    # 浏览器授权
cloudflared tunnel create llm-wiki
cloudflared tunnel route dns llm-wiki wiki.example.com

# /etc/cloudflared/config.yml
# tunnel: <TUNNEL_ID>
# credentials-file: /root/.cloudflared/<TUNNEL_ID>.json
# ingress:
#   - hostname: wiki.example.com
#     service: http://127.0.0.1:8081
#   - service: http_status:404

systemctl enable --now cloudflared
```

`wiki.example.com` DNS 在 Cloudflare 即可，**不需要**国内备案（仅当 Cloudflare 走海外 edge 时；若要国内访问需 ICP）。

---

## 10. 相关文档

| 文档 | 说明 |
|------|------|
| [低配机交叉编译CLI.md](./低配机交叉编译CLI.md) | musl 静态构建的详细步骤 |
| [部署-ECS与Tunnel.md](./部署-ECS与Tunnel.md) | 通用 ECS + Cloudflare Tunnel 流程 |
| [部署指引.md](./部署指引.md) | 部署选型与检查清单 |
| [开发与测试.md](./开发与测试.md) | 本地构建 / e2e / FAQ |
| [overlay/config/README.md](../overlay/config/README.md) | server config 字段说明 |
| [overlay/patches/README.md](../overlay/patches/README.md) | patch 维护 |
