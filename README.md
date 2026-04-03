# tweet-db

Centralized post capture storage and status service built with Axum, PostgreSQL, and Vue.

The expected integration model is a Tampermonkey script using `GM_*` privileged network APIs as a centralized client for this service. Public API consumers only need one session-check endpoint plus post ingest/query endpoints; login and first-time registration are handled separately through the built-in `/account` SPA.

## API Docs

- [`docs/public-api.md`](docs/public-api.md): public business API under `/api/v1/...`
- [`docs/internal-api.md`](docs/internal-api.md): internal-only API under `/internal/v1/...`
- [`docs/integrations.md`](docs/integrations.md): external integration entrypoints under `/integrations/...`

## Layout

- `server/`: Rust backend and database migrations
- `webui/`: Vue frontend for `/account` plus the internal `/admin` management console

## Server TLS

The backend server now supports protocol selection through the `server` section in TOML configuration:

- `server.mode = "http"` keeps the current plaintext listener behavior.
- `server.mode = "https"` requires `server.tls.certificate_chain_path` and `server.tls.private_key_path`, both resolved relative to the loaded TOML file unless absolute paths are used.

When `server.mode = "https"`, startup fails fast unless `app.base_url` and `sso.login_redirect_uri` use `https://`, and `session.cookie_secure = true`.
