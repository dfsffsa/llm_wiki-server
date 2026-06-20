# smhanov-auth-demo

Minimal runnable demo for the upstream [`smhanov/auth`](https://github.com/smhanov/auth) source tree.

> 原 demo 工作目录是 `~/hn_low_cost_site_analysis_2026-04-15/smhanov-auth-demo/`，此处只保留它的 README 作为接入示例参考。源码本身未一并归档（开源库 + 一次性 demo，无需冗余复制）。

## What It Shows

- SQLite-backed `smhanov/auth` integration
- `/user/*` endpoints mounted directly
- Thin `/auth/*` JSON adapter layered on top
- Cookie-based login session
- Register / login / logout / whoami flow
- Forgot-password flow without SMTP

Instead of sending a real password-reset email, the demo captures the latest reset URL in memory and exposes it at:

- `GET /app/debug/reset-link`

## Run

```bash
# 原工作目录（已不在仓库中）
# cd ~/hn_low_cost_site_analysis_2026-04-15/smhanov-auth-demo
go run .
```

Then open:

- `http://127.0.0.1:8081`

## Endpoints

Native `smhanov/auth` routes:

- `POST /user/create`
- `POST /user/auth`
- `GET|POST /user/signout`
- `GET /user/get`
- `POST /user/forgotpassword`
- `POST /user/resetpassword`

Thin JSON adapter routes:

- `POST /auth/register`
- `POST /auth/login`
- `POST /auth/logout`
- `GET /auth/me`
- `POST /auth/forgot-password`
- `POST /auth/reset-password`

Example:

```bash
curl -c cookies.txt -b cookies.txt \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","password":"secret123","sign_in":true}' \
  http://127.0.0.1:8081/auth/register

curl -c cookies.txt -b cookies.txt http://127.0.0.1:8081/auth/me
```

## Files

- `main.go`: demo server and HTML UI
- `demo.sqlite3`: created on first run
