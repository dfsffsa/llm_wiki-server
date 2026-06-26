# 邮件配置（SMTP / Resend）

> 密码重置、（未来）注册确认、订单/告警邮件都走 SMTP。代码是 provider 无关的（[`overlay/server/src/mail.rs`](../overlay/server/src/mail.rs)），任何 SMTP 服务器都能接。本文以 **Resend** 为例——免费档 3000 封/月、5 分钟拿到凭据、送达率高，是商用起步首选。

## 0. 先理解：代码不改，只填配置 + 配 DNS

- **SMTP 客户端已在代码里**（lettre crate）。`forgot-password` 检测到 `smtp` 配置块就真发邮件，否则回退到日志打印 token（开发态）。
- **你要做的只有两件事**：(1) 注册 Resend 拿 SMTP 凭据；(2) 在你的域名 DNS 配 SPF/DKIM/DMARC。
- **"正规"的关键全在 DNS**——这决定邮件进收件箱还是垃圾箱。Resend 只是把邮件投出去，能不能被 Gmail/QQ/Outlook 收下，看你的 DNS 配得对不对。

## 1. 注册 Resend + 拿 API Key

1. [resend.com](https://resend.com) 注册（GitHub 登录即可）
2. Dashboard → **API Keys** → **Add API Key** → 复制 `re_xxxxxxxxxxxxxxxxxxxxxxxxx`（**只显示一次，存好**）
3. 免费档：3000 封/月、100 封/天，够起步商用

## 2. 验证发信域名（"正规"的核心，必须做）

**不要用 `@resend.dev`、`@gmail.com`、`@qq.com` 发信**——必进垃圾箱。用你自己拥有的域名。

Resend Dashboard → **Domains** → **Add Domain** → 填域名。

> **强烈建议用专用发信子域**：填 `mail.yourdomain.com` 而非主域 `yourdomain.com`。子域能隔离发信信誉——主域的邮件（你日常通信）不受发信波动影响，专业做法。

Resend 会给你几条 DNS 记录，**逐条加到你域名的 DNS 解析里**（在域名注册商/Cloudflare/阿里云 DNS 那里加）：

| 记录类型 | 名称（示例，以主域为例） | 值（Resend 实时给出，每账号不同） | 作用 |
|---|---|---|---|
| TXT | `resend._domaincheck.yourdomain.com` | Resend 给的一串 | 域名所有权验证 |
| CNAME | `resend._domainkey.yourdomain.com` | Resend 给的 DKIM 值 | **DKIM：邮件签名，防伪造，最关键** |
| TXT | `yourdomain.com` | `v=spf1 include:_spf.resend.com ~all` | **SPF：授权 Resend 代你发信** |
| TXT | `_dmarc.yourdomain.com` | `v=DMARC1; p=none; rua=mailto:dmarc@yourdomain.com` | **DMARC：先观察模式** |

> 用子域 `mail.yourdomain.com` 时，所有记录名换成该子域（如 SPF 加在 `mail.yourdomain.com` 上）。

加完回 Resend 点 **Verify**，通常几分钟到几小时通过（DNS 全球生效需要时间）。

**SPF + DKIM + DMARC 三件套齐全 = 正规**。收件方校验这三项通过才进收件箱，否则大概率垃圾箱或直接拒收。

### DMARC 渐进收紧策略

| 阶段 | `p=` 值 | 含义 | 时机 |
|---|---|---|---|
| 起步 | `p=none` | 只观察、不处置 | 刚配好，跑 1-2 周 |
| 稳定 | `p=quarantine` | 伪冒邮件进垃圾箱 | `rua` 报告显示无异常伪冒后 |
| 最终 | `p=reject` | 伪冒邮件直接拒收 | 信誉稳定后 |

`rua=mailto:dmarc@yourdomain.com` 收 DMARC 报告，可用 [Postmark DMARC Digest](https://dmarc.postmarkapp.com/) 免费解析报告，看是否有伪冒。

## 3. 填写 smtp 配置

把 `server.local.json`（或 `overlay/config/server.example.json` 模板）的 `smtp` 块填成真实值。Resend 的 SMTP 参数是固定的：

```json
"smtp": {
  "enabled": true,
  "host": "smtp.resend.com",
  "port": 587,
  "user": "resend",
  "pass": "${SMTP_PASS}",
  "from": "LLM Wiki <noreply@yourdomain.com>",
  "publicBaseUrl": "https://wiki.yourdomain.com"
}
```

字段说明：

| 字段 | 值 | 说明 |
|---|---|---|
| `host` | `smtp.resend.com` | Resend 固定主机 |
| `port` | `587` | STARTTLS（代码默认，兼容） |
| `user` | `resend` | **字面量 `resend`**，不是你的邮箱（Resend 规定） |
| `pass` | `${SMTP_PASS}` | 用占位符，环境变量注入真实 API Key `re_...`（见下）。**不要写死进 JSON** |
| `from` | `LLM Wiki <noreply@yourdomain.com>` | 格式 `显示名 <本地部分@你的已验证域名>`。域名必须与第 2 步验证一致，否则 Resend 拒发 |
| `publicBaseUrl` | `https://wiki.yourdomain.com` | 站点公网地址，用于拼重置链接 `https://wiki.yourdomain.com/reset-password?token=...` |

### 密钥注入（systemd）

跟 `LLM_API_KEY` 同一套 `${VAR}` 占位符机制（[`config.rs` 的 `expand_env_placeholders`](../overlay/server/src/config.rs)）。在 systemd unit 注入：

```ini
# /etc/systemd/system/llm-wiki-server.service
[Service]
Environment=SMTP_PASS=re_xxxxxxxxxxxxxxxxxxxxxxxxx
# ... 其它 Environment=
```

```bash
systemctl daemon-reload && systemctl restart llm-wiki-server
```

`server.local.json` 本身 chmod 600 + gitignore（`*.local.json`），但密钥走环境变量更安全——配置文件只留占位符。

## 4. 启动 + 实测

```bash
# 1. 确认健康
curl -sS http://127.0.0.1:8080/api/v1/health?deep=true | jq .authDb
# → "ok"

# 2. 用真实邮箱注册一个测试账号
curl -X POST http://127.0.0.1:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"你的真实邮箱@gmail.com","password":"password1"}'

# 3. 触发重置
curl -X POST http://127.0.0.1:8080/auth/forgot-password \
  -H 'Content-Type: application/json' \
  -d '{"email":"你的真实邮箱@gmail.com"}'
# → {"ok":true}  （无论是否发出都回 ok，防邮箱枚举）
```

- **成功**：Gmail 收到 `重置你的 LLM Wiki 密码` 邮件，**进收件箱**（非垃圾箱）
- **失败查日志**：`journalctl -u llm-wiki-server -n 50 | grep -i "password-reset\|smtp"`
- **进垃圾箱**：多半是 DMARC 没配或 `from` 域名没验证，回第 2 步检查 DNS；用 [mail-tester.com](https://www.mail-tester.com/) 发一封自测能打分

## 5. 排错速查

| 现象 | 原因 | 解决 |
|---|---|---|
| 日志 `Connection refused` | SMTP 端口不通 | 检查 `host`/`port`；ECS 出网 587 是否被防火墙拦 |
| 日志 `535 Authentication failed` | API Key 错或 `user` 不是 `resend` | 核对 `SMTP_PASS` 和 `user:"resend"` |
| 日志 `421 Domain not verified` | 域名没在 Resend 验证 | 回第 2 步加 DNS + Verify |
| 邮件进垃圾箱 | SPF/DKIM/DMARC 不全或 `from` 域名不匹配 | 用 mail-tester.com 诊断；补 DNS 三件套 |
| `from` 报错 `sender not allowed` | `from` 域名 ≠ 已验证域名 | `from` 的 `@` 后部分必须 = Resend 已验证域名 |
| 收不到、日志却无错 | Resend 已接收但投递延迟 | Dashboard → Logs 看投递状态 |

## 6. "显得正规"的加分项

1. **专用发信子域**（`mail.yourdomain.com`）——主域信誉不受发信波动影响
2. **发信地址语义化**：`noreply@`（系统邮件，不期待回复）/ `hello@`（可回复），别用 `admin@`/`test@`
3. **DMARC 渐进收紧**：`none → quarantine → reject`，看 `rua` 报告数据驱动
4. **监控送达率**：Resend Dashboard 看 Bounces/Complaints，**投诉率 <0.1%** 才健康；超 0.1% 会被各家限流
5. **不在正文写敏感信息**：当前邮件正文只有重置链接 + TTL 说明（代码已实现），不回传密码、不附 token 明文之外的凭据

## 7. 换用其它 SMTP 服务商

代码 provider 无关，换哪家只改 `host`/`user`/`pass`，DNS 三件套每家都要配（记录值由各家提供）：

| 服务商 | host | user | 免费档 | 备注 |
|---|---|---|---|---|
| Resend | `smtp.resend.com` | `resend` | 3000/月 | 本文示例 |
| Mailgun | `smtp.mailgun.org` | `postmaster@mg.你的域名` | 5000/月 | 老牌稳定 |
| Brevo (SendinBlue) | `smtp-relay.brevo.com` | 你的 login 邮箱 | 300/天 | 国内可达 |
| AWS SES | `email-smtp.<region>.amazonaws.com` | SES SMTP 用户名 | 62000/月（仅 EC2 内） | 需沙箱解封 |
| 阿里云邮件推送 | `smtpdm.aliyun.com` | 控制台生成 | 按量 | 国内送达率好 |
| 腾讯企业邮 | `smtp.exmail.qq.com` | 企业邮箱地址 | 套餐内 | 已有企业邮可复用 |

Gmail/163/QQ 个人邮箱 SMTP 仅适合测试（几十封/天，量大封号），**不要用于商用**。

## 8. 不配 SMTP 时的行为

`smtp` 块缺失或 `enabled:false` → `forgot-password` 把重置 token 打印到 server 日志（`journalctl`），运维手动把链接给用户。适合开发/内测，**不适合正式商用**——用户拿不到邮件。

## 9. 相关代码与文档

- 实现：[`overlay/server/src/mail.rs`](../overlay/server/src/mail.rs)（SMTP 客户端 + 重置链接构造，7 个单测）
- 调用点：[`overlay/server/src/api/auth_routes.rs`](../overlay/server/src/api/auth_routes.rs) `handle_forgot`
- 配置解析：[`mail::parse_smtp_config`](../overlay/server/src/mail.rs)（读 `smtp` 块）
- 配置模板：[`overlay/config/server.example.json`](../overlay/config/server.example.json)（含 smtp 示例）
- 密钥注入与 `${VAR}` 展开：[`config.rs`](../overlay/server/src/config.rs)
- 部署 systemd unit：[`docs/部署-低配ECS一键脚本.md`](./部署-低配ECS一键脚本.md)
