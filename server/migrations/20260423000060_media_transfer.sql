CREATE TYPE media.transfer_status AS ENUM ('pending', 'processing', 'completed', 'failed');

CREATE TABLE media.storage_object (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL,
    bucket TEXT NOT NULL,
    object_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    content_length BIGINT NOT NULL CHECK (content_length >= 0),
    etag TEXT,
    sha256_hex TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_storage_object_provider_bucket_key UNIQUE (provider, bucket, object_key)
);

CREATE TABLE media.transfer_task (
    id UUID PRIMARY KEY,
    media_id BIGINT NOT NULL,
    source_recorded_at TIMESTAMPTZ NOT NULL,
    source_url TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_content_type TEXT,
    status media.transfer_status NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    storage_object_id UUID REFERENCES media.storage_object(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_transfer_task_source_resource
        FOREIGN KEY (media_id, source_recorded_at)
        REFERENCES tweet.media_resource(media_id, recorded_at)
        ON DELETE CASCADE,
    CONSTRAINT uq_transfer_task_media_resource UNIQUE (media_id, source_recorded_at)
);

CREATE INDEX idx_transfer_task_status_created_at
ON media.transfer_task (status, created_at, id);

CREATE INDEX idx_transfer_task_processing_claimed_at
ON media.transfer_task (claimed_at, id)
WHERE status = 'processing';

CREATE INDEX idx_transfer_task_media_updated_at
ON media.transfer_task (media_id, updated_at DESC, id DESC);

CREATE INDEX idx_transfer_task_storage_object_id
ON media.transfer_task (storage_object_id)
WHERE storage_object_id IS NOT NULL;

CREATE VIEW media.v_latest_transfer_overview AS
SELECT DISTINCT ON (task.media_id)
    task.media_id,
    task.id AS transfer_task_id,
    task.source_recorded_at,
    task.source_url,
    task.source_kind,
    task.source_content_type,
    task.status,
    task.attempt_count,
    task.last_error,
    task.claimed_by,
    task.claimed_at,
    task.completed_at,
    task.created_at AS task_created_at,
    task.updated_at AS task_updated_at,
    object.id AS storage_object_id,
    object.provider,
    object.bucket,
    object.object_key,
    object.content_type,
    object.content_length,
    object.etag,
    object.sha256_hex,
    object.created_at AS storage_object_created_at
FROM media.transfer_task AS task
LEFT JOIN media.storage_object AS object
  ON object.id = task.storage_object_id
ORDER BY
    task.media_id,
    COALESCE(task.completed_at, task.updated_at, task.created_at) DESC,
    task.created_at DESC,
    task.id DESC;
