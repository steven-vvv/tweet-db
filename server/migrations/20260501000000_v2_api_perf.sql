CREATE INDEX IF NOT EXISTS idx_transfer_task_media_completed_lookup
ON media.transfer_task (media_id, status, completed_at DESC, updated_at DESC, id DESC)
WHERE status = 'completed' AND storage_object_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_user_sso_authorizations_created_lookup
ON iam.user_sso_authorizations (user_id, created_at DESC, authorization_id DESC);

CREATE INDEX IF NOT EXISTS idx_sessions_created_lookup
ON iam.sessions (user_id, created_at DESC, selector DESC);

CREATE INDEX IF NOT EXISTS idx_audit_events_user_lookup
ON audit.audit_events (resource_type, resource_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_index_queue_updated_lookup
ON search.index_queue (updated_at DESC, id DESC);
