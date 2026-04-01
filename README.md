# tweet-db

Centralized post capture storage and status service built with Axum, PostgreSQL, and Vue.

## API Docs

- [`docs/public-api.md`](docs/public-api.md): public business API under `/api/v1/...`
- [`docs/internal-api.md`](docs/internal-api.md): internal-only API under `/internal/v1/...`
- [`docs/integrations.md`](docs/integrations.md): external integration entrypoints under `/integrations/...`

## Layout

- `server/`: Rust backend and database migrations
- `webui/`: minimal Vue frontend for login, registration, and account status
