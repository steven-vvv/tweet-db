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
