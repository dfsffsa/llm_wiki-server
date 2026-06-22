# 在低配服务器上部署 CLI：本地交叉编译 musl 静态二进制

> 适用场景：服务器内存 ≤ 2GB / 磁盘 I/O 差，**无法**直接在服务器上 `cargo build`（链接 lance/lancedb 阶段易 OOM 或耗时数小时）。
> 思路：在性能更好的本地机器上编译出**静态二进制**，scp 到服务器即可使用。
> 全局文档入口：[文档指引.md](../文档指引.md) · 相关：[开发与测试.md](./开发与测试.md) · [部署指引.md](./部署指引.md)

---

## 1. 为什么选 musl 静态

| 维度 | 普通 glibc 动态 | musl 静态 |
|------|----------------|----------|
| 依赖 | 受目标机 glibc 版本限制 | 无任何外部依赖 |
| 兼容 | 需构建机 glibc ≤ 目标机 glibc | 任意 x86_64 Linux 都能跑（包括 Alpine、CentOS 7） |
| 体积 | release 30–50 MB | release 40–60 MB（多 ~30%） |
| 性能 | 标准 | malloc 密集场景慢 1–5%；本 CLI 一次性命令无感 |
| 本项目兼容性 | ✅ | ✅（无 OpenSSL，用 rustls；LanceDB / Arrow / DataFusion 全 Rust） |

CLI 是"一次性命令"（search/ingest/reindex 等），性能损失可忽略，**强烈推荐 musl 静态**。

---

## 2. 本地构建机要求

- **架构**：x86_64 Linux（Linux Mint / Ubuntu / Debian / Fedora 均可）
- **内存**：建议 ≥ 4 GB（链接 lance 时峰值 ~1.5 GB）
- **磁盘**：≥ 10 GB（target 目录约 4–5 GB）
- **网络**：能访问 crates.io 或国内镜像

> 也可在 macOS / Windows 用 Docker 跨编译，思路相同。本文以 Linux Mint / Ubuntu 为例。

---

## 3. 一次性环境准备

### 3.1 装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source $HOME/.cargo/env
rustc --version    # 1.75+ 即可
```

### 3.2 添加 musl target + 工具链

```bash
rustup target add x86_64-unknown-linux-musl
sudo apt update
sudo apt install -y musl-tools protobuf-compiler
musl-gcc --version
protoc --version   # 需 ≥ 3.21；Mint 仓库版本若太老见下方 3.3
```

### 3.3 protoc 版本不够时手动安装

LanceDB 要求 protoc 3.21+。Ubuntu 22.04 / Mint 21 仓库的 `protobuf-compiler` 是 3.12，会失败。手动装：

```bash
PROTOC_VER=28.3
curl -fsSL -o /tmp/protoc.zip \
  "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VER}/protoc-${PROTOC_VER}-linux-x86_64.zip"
sudo unzip -o /tmp/protoc.zip -d /usr/local
protoc --version    # libprotoc 28.3
rm /tmp/protoc.zip
```

### 3.4 国内网络加速（可选）

如果 crates.io 直连慢/失败，配置清华镜像：

```bash
mkdir -p ~/.cargo
cat > ~/.cargo/config.toml << 'EOF'
[source.crates-io]
replace-with = "tuna"

[source.tuna]
registry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"

[net]
retry = 10
EOF
```

---

## 4. 克隆代码

```bash
git clone --recurse-submodules <你的 git 仓库 URL> llm_wiki-server
cd llm_wiki-server
```

如已克隆未拉子模块：

```bash
git submodule update --init --recursive
```

---

## 5. 配置 musl 链接器

仓库根目录新建 `.cargo/config.toml`（**不要提交此文件**，已加 .gitignore；本机一次性配置即可）：

```bash
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.x86_64-unknown-linux-musl]
linker = "musl-gcc"

[net]
retry = 10
EOF
```

> 如果你已经在 `~/.cargo/config.toml` 用了清华镜像，本地 `.cargo/config.toml` 会被自动合并，互不冲突。

---

## 6. 编译

```bash
# release 模式 + musl target，CPU 全开
cargo build --release \
  --manifest-path overlay/cli/rust/Cargo.toml \
  --target x86_64-unknown-linux-musl
```

预计耗时（4 核 / 8GB 笔电）：**5–15 分钟**（首次）。失败重试 `cargo build` 即可，crate 缓存会复用。

---

## 7. 验证产物是否真为静态

```bash
BIN=overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki

ls -lh "$BIN"
file "$BIN"
# 期望: ELF 64-bit LSB executable, x86-64, ..., statically linked, ...

ldd "$BIN"
# 期望: not a dynamic executable
```

如果看到 `dynamically linked` 或 `ldd` 列出依赖，说明链接到了 glibc，**部署到老服务器会报 GLIBC 版本错误**。检查 `.cargo/config.toml` 是否生效。

可选本机试跑：

```bash
"$BIN" --help
```

---

## 8. 上传到服务器

```bash
SERVER=root@your-server         # 改成你的服务器
SERVER_REPO=/root/llm_wiki-server   # 改成服务器上的仓库路径

# 1) 确保服务器接收目录存在
ssh "$SERVER" "mkdir -p $SERVER_REPO/overlay/cli/rust/target/release"

# 2) 上传
scp overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki \
    "$SERVER:$SERVER_REPO/overlay/cli/rust/target/release/llm-wiki"

# 3) 服务器侧赋权
ssh "$SERVER" "chmod +x $SERVER_REPO/overlay/cli/rust/target/release/llm-wiki"
```

> `scripts/llm-wiki` wrapper 默认查 `overlay/cli/rust/target/release/llm-wiki`，所以放这里即可，**不要**放 `target/debug/` 或 musl target 的子目录。

---

## 9. 服务器侧补全 Node CLI 依赖

CLI 中的 `ingest` / `reindex --vectors` 是 Node/TS 实现，**不需要交叉编译**。在服务器上一次性装即可（速度快、不爆磁盘）：

```bash
ssh "$SERVER"
cd /root/llm_wiki-server
npm install --prefix overlay/cli/node
```

如果服务器无 npm/Node，参考 [开发与测试.md §3](./开发与测试.md) 装 nvm + Node。

---

## 10. 服务器侧验证

```bash
cd /root/llm_wiki-server

./scripts/llm-wiki --help
./scripts/llm-wiki search "test" --project /path/to/your/wiki --top-k 3
./scripts/llm-wiki rescan --project /path/to/your/wiki --json
```

如果 `--help` 正常输出，说明二进制能跑、glibc 兼容性 OK。

---

## 11. 后续升级

当 `overlay/cli/rust/` 或 `overlay/crates/llm-wiki-common/` 有改动时：

```bash
# 本地
cd llm_wiki-server
git pull --recurse-submodules
cargo build --release \
  --manifest-path overlay/cli/rust/Cargo.toml \
  --target x86_64-unknown-linux-musl

# 上传
scp overlay/cli/rust/target/x86_64-unknown-linux-musl/release/llm-wiki \
    "$SERVER:$SERVER_REPO/overlay/cli/rust/target/release/llm-wiki"
```

增量编译，通常只需 1–3 分钟。

---

## 12. 同样适用于 server

`overlay/server/` 也是 Rust 项目，同样的方法：

```bash
cargo build --release \
  --manifest-path overlay/server/Cargo.toml \
  --target x86_64-unknown-linux-musl

# 产物
ls -lh overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server

# 上传
scp overlay/server/target/x86_64-unknown-linux-musl/release/llm-wiki-server \
    "$SERVER:$SERVER_REPO/overlay/server/target/release/llm-wiki-server"
```

> 注意：server **不依赖 lancedb**，编译比 CLI 快得多（通常 2–5 分钟），即使在 2GB 内存机上也能直接编。但用 musl 静态版本仍然方便分发。

---

## 13. 常见问题

| 现象 | 处理 |
|------|------|
| `error: linker 'musl-gcc' not found` | `sudo apt install musl-tools` |
| `Could not find protoc` 或 `protoc out of date` | 按 §3.3 装 protoc 28.3+ |
| `error: failed to run custom build command for 'ring'` 或 `aws-lc-sys` | 极少；加 `RUSTFLAGS="-C target-feature=-crt-static"` 重试 |
| `download of xx failed` `[28] Timeout` | crates.io 直连慢，按 §3.4 配清华镜像 |
| 服务器跑二进制报 `GLIBC_X.XX not found` | 没编成 musl 静态。`file` / `ldd` 检查二进制；通常是 `--target` 漏写 |
| 编译 OOM（笔电也不够） | 加 `--jobs 2`；或加 swap；或临时去掉 `lancedb` 依赖（牺牲向量功能） |
| `Permission denied` 跑二进制 | 服务器侧没 `chmod +x` |

---

## 14. 为什么不在服务器上直接编

参考记录（2026-06）：1.6GB 内存 / 2 vCPU / 磁盘 I/O 差的服务器上：

- crate 下载阶段：受国外网络影响，多次超时，需国内镜像
- 编译阶段：单线程 jobs=1 + ionice idle 避免拖死系统
- 链接阶段：`rust-lld` 链接 lancedb + arrow + datafusion 时内存峰值 ~1.5GB，**临界 OOM**，会被内核 kill 而无明显报错
- 一次完整编译耗时：≥ 6 小时（crate 下载 + 编译 + 链接），且容易因 OOM 反复失败

本地 4 核 / 8GB 笔电，**5–15 分钟**完成，scp 上传 30 秒。

---

## 15. 一键部署到低配 ECS

上面 §1–§14 讲的是**怎么编出** musl 静态二进制。第 8 / 12 节的 scp 上传是手工步骤，每次重新构建后都要重做一遍。

仓库提供 `./scripts/deploy-ecs.sh` 把**整套**（server + CLI + `upstream/dist` + `upstream/src` + `overlay/cli/node` + `server.local.json` + systemd）打成一次部署。SSH target、端口、远端路径、API key 全走 env，常用一行：

```bash
SSH_HOST=root@47.103.39.152 SSH_PORT=22022 SERVER_PORT=8081 \
LLM_API_KEY='sk-...' ./scripts/deploy-ecs.sh
```

完整参数、坑位速查（vite alias 顺序、tsx 是 devDep、upstream/node_modules 必要性等）见 [部署-低配ECS一键脚本.md](./部署-低配ECS一键脚本.md)。

---

## 16. 相关文档

| 文档 | 说明 |
|------|------|
| [部署-低配ECS一键脚本.md](./部署-低配ECS一键脚本.md) | 一键部署脚本使用 + 坑位速查 |
| [部署-ECS与Tunnel.md](./部署-ECS与Tunnel.md) | 通用 ECS + Cloudflare Tunnel |
| [开发与测试.md](./开发与测试.md) | 本地构建 / e2e / FAQ |
