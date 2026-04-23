# tweet-db

Centralized post capture storage and status service built with Axum, PostgreSQL, and Vue.

The expected integration model is a Tampermonkey script using `GM_*` privileged network APIs as a centralized client for this service. Public API consumers use the session-check endpoint plus batch tweet submit/query endpoints; query requires a registered session, while submit is restricted to admin sessions. Login and first-time registration are handled separately through the built-in `/account` SPA.

## API Docs

- [`docs/public-api.md`](docs/public-api.md): public business API under `/api/v1/...`
- [`docs/internal-api.md`](docs/internal-api.md): internal-only API under `/internal/v1/...`
- [`docs/integrations.md`](docs/integrations.md): external integration entrypoints under `/integrations/...`

## Product Docs

- [`docs/product-requirements.md`](docs/product-requirements.md): product feature scope, roles, and business requirements

## Design Docs

- [`docs/tweet-v2-schema-design.md`](docs/tweet-v2-schema-design.md): tweet v2 table relations and write policy
- [`docs/tweet-v2-schema-notes.md`](docs/tweet-v2-schema-notes.md): tweet v2 schema organization notes
- [`docs/media-transfer-lifecycle.md`](docs/media-transfer-lifecycle.md): media transfer queue, worker lifecycle, and local verification flow

## Layout

- `server/`: Rust backend and database migrations
- `webui/`: Vue frontend for `/account` plus the internal `/admin` management console

## Server TLS

The backend server now supports protocol selection through the `server` section in TOML configuration:

- `server.mode = "http"` keeps the current plaintext listener behavior.
- `server.mode = "https"` requires `server.tls.certificate_chain_path` and `server.tls.private_key_path`, both resolved relative to the loaded TOML file unless absolute paths are used.

When `server.mode = "https"`, startup fails fast unless `app.base_url` and `sso.login_redirect_uri` use `https://`, and `session.cookie_secure = true`.
