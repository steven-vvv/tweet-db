ALTER TYPE media.transfer_status ADD VALUE IF NOT EXISTS 'canceled';

CREATE INDEX idx_twitter_user_updated_at
ON tweet.twitter_user (updated_at DESC, id DESC);

CREATE INDEX idx_media_updated_at
ON tweet.media (updated_at DESC, id DESC);

CREATE INDEX idx_storage_object_created_at
ON media.storage_object (created_at DESC, id DESC);

CREATE INDEX idx_storage_object_object_key_prefix
ON media.storage_object (object_key text_pattern_ops);

CREATE INDEX idx_transfer_task_updated_at
ON media.transfer_task (updated_at DESC, id DESC);

CREATE INDEX idx_user_snapshot_user_name_prefix
ON tweet.user_snapshot (lower(user_name) text_pattern_ops, recorded_at DESC);

CREATE INDEX idx_user_snapshot_display_name_prefix
ON tweet.user_snapshot (lower(display_name) text_pattern_ops, recorded_at DESC);
