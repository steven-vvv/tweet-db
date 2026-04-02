CREATE INDEX idx_storage_objects_created_at
ON storage_objects (created_at DESC, id DESC);

CREATE INDEX idx_media_transfer_jobs_updated_at
ON media_transfer_jobs (updated_at DESC, id DESC);
