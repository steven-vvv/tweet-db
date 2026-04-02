DROP INDEX IF EXISTS idx_posts_author_source_actor_id;

CREATE INDEX idx_posts_author_source_actor_recent
ON posts (source_kind, author_source_actor_id, last_observed_at DESC, source_post_id DESC);

CREATE INDEX idx_post_media_sources_managed_media_recent
ON post_media_sources (managed_media_id, last_observed_at DESC, source_media_id DESC);

CREATE INDEX idx_media_storage_bindings_storage_object_created_at
ON media_storage_bindings (storage_object_id, created_at DESC, id DESC);

CREATE INDEX idx_audit_events_resource_created_at
ON audit_events (resource_type, resource_id, created_at DESC, id DESC);

CREATE INDEX idx_media_transfer_attempts_started_at
ON media_transfer_attempts (started_at DESC, id DESC);

DROP INDEX IF EXISTS idx_actor_profile_versions_current;

CREATE UNIQUE INDEX uq_actor_profile_versions_current
ON actor_profile_versions (source_kind, source_actor_id)
WHERE effective_to IS NULL;
