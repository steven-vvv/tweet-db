# tweet-db

`tweet-db` 是一个保存 X 帖子、查询同步状态、转储媒体文件的服务。后端使用 Axum 和 PostgreSQL，前端使用 Vue。

主要调用方是浏览器脚本，例如使用 `GM_*` 网络能力的 Tampermonkey 脚本。外部调用方可以检查会话、批量提交 tweet v2 数据、批量查询已保存数据。查询需要已注册会话，提交需要管理员会话。登录和首次绑定账号通过内置 `/account` 页面完成。

## 目录

- `server/`：Rust 后端、数据库迁移、媒体转储 worker。
- `webui/`：Vue 前端，包含 `/account` 和 `/admin` 页面。
- `docs/`：当前项目文档。

## 文档

- [`docs/api.md`](docs/api.md)：HTTP API。
- [`docs/architecture.md`](docs/architecture.md)：服务端、数据库、tweet 写入和媒体转储说明。
- [`docs/roadmap.md`](docs/roadmap.md)：已完成内容、未完成内容和后续开发约束。

## 本地检查

在 `server/` 下运行：

```bash
cargo test
cargo fmt -- --check
```

在 `webui/` 下运行：

```bash
bun run type
bun run build
```

## HTTPS 配置

后端通过 TOML 配置中的 `[server]` 选择 HTTP 或 HTTPS：

- `server.mode = "http"`：使用普通 HTTP。
- `server.mode = "https"`：需要配置 `server.tls.certificate_chain_path` 和 `server.tls.private_key_path`。相对路径按当前 TOML 文件所在目录解析。

启用 HTTPS 时，`app.base_url` 和 `sso.login_redirect_uri` 必须使用 `https://`，并且 `session.cookie_secure = true`，否则服务会启动失败。
