# auth

> 这份文档是上游开源库 `smhanov/auth` 的 README 归档副本，源码出处：
>
> - 上游仓库：<https://github.com/smhanov/auth>
> - Go 文档：<https://pkg.go.dev/github.com/smhanov/auth>
> - License：MIT（见上游仓库 `LICENSE`）
>
> 本仓库未冗余复制源码，需要阅读完整代码请直接 `git clone https://github.com/smhanov/auth`。

Package `auth` provides a complete, self-hosted user authentication system for Go web applications.

[![Go Reference](https://pkg.go.dev/badge/github.com/smhanov/auth.svg)](https://pkg.go.dev/github.com/smhanov/auth)

## Overview

This library aims to provide "boring" but essential user authentication infrastructure, saving you from rewriting the same login logic for every project. It is designed to be dropped into your existing web application with minimal configuration.

## Key Benefits

- **Comprehensive**: Handles Email/Password, OAuth (Google, Facebook, Twitter), and Enterprise SAML SSO out of the box.
- **Secure**: Includes built-in rate limiting, secure session management, and password hashing standards.
- **Self-Hosted**: You own your data. Supports SQLite and PostgreSQL via `sqlx`.
- **Complete Flows**: Includes ready-to-use flows for password resets, email updates, and account creation.

## Documentation

For full documentation, tutorials, and API reference, please visit the official Go docs:

**[https://pkg.go.dev/github.com/smhanov/auth](https://pkg.go.dev/github.com/smhanov/auth)**

## Installation

```shell
go get github.com/smhanov/auth
```
