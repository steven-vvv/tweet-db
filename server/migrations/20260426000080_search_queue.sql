CREATE SCHEMA IF NOT EXISTS search;
COMMENT ON SCHEMA search IS 'Local Tantivy search indexes, indexing queue, and operational state.';

CREATE TYPE search.index_target_kind AS ENUM ('user', 'tweet');
CREATE TYPE search.index_task_status AS ENUM ('pending', 'processing', 'completed', 'failed');

CREATE TABLE search.index_queue (
    id UUID PRIMARY KEY,
    target_kind search.index_target_kind NOT NULL,
    target_id BIGINT NOT NULL,
    status search.index_task_status NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_index_queue_target UNIQUE (target_kind, target_id)
);

CREATE INDEX idx_index_queue_status_created_at
ON search.index_queue (status, created_at, id);

CREATE INDEX idx_index_queue_processing_claimed_at
ON search.index_queue (claimed_at, id)
WHERE status = 'processing';

CREATE INDEX idx_index_queue_target_status
ON search.index_queue (target_kind, status, target_id);
