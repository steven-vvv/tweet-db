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

CREATE TABLE media_transfer_jobs (
    id UUID PRIMARY KEY,
    media_id UUID NOT NULL REFERENCES managed_media(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    fetch_url TEXT NOT NULL,
    content_type_hint TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'retryable', 'succeeded', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    leased_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    storage_object_id UUID REFERENCES storage_objects(id) ON DELETE SET NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (media_id)
);

CREATE INDEX idx_media_transfer_jobs_status_schedule
ON media_transfer_jobs (status, next_run_at, lease_expires_at);

CREATE TABLE media_transfer_attempts (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES media_transfer_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    upload_mode TEXT NOT NULL CHECK (upload_mode IN ('single_put', 'multipart')),
    error TEXT,
    bytes_uploaded BIGINT NOT NULL DEFAULT 0,
    parts_uploaded INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX idx_media_transfer_attempts_job_id_started_at
ON media_transfer_attempts (job_id, started_at DESC);

CREATE TABLE media_storage_bindings (
    id UUID PRIMARY KEY,
    media_id UUID NOT NULL REFERENCES managed_media(id) ON DELETE CASCADE,
    storage_object_id UUID NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    object_role TEXT NOT NULL DEFAULT 'original',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (media_id, object_role)
);
