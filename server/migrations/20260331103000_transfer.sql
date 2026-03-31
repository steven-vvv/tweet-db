CREATE TABLE storage_objects (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL,
    bucket TEXT NOT NULL,
    object_key TEXT NOT NULL UNIQUE,
    etag TEXT,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE transfer_jobs (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    source_post_id TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'retryable', 'succeeded', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    leased_at TIMESTAMPTZ,
    storage_object_id UUID REFERENCES storage_objects(id) ON DELETE SET NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_media_id)
);

CREATE INDEX idx_transfer_jobs_status_next_run_at
ON transfer_jobs (status, next_run_at);

CREATE TABLE transfer_attempts (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES transfer_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    error TEXT,
    bytes_uploaded BIGINT NOT NULL DEFAULT 0,
    parts_uploaded INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX idx_transfer_attempts_job_id_started_at
ON transfer_attempts (job_id, started_at DESC);

CREATE TABLE media_storage_bindings (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    storage_object_id UUID NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    variant_role TEXT NOT NULL DEFAULT 'primary',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_media_id, variant_role)
);
