CREATE INDEX idx_actor_metric_observations_actor_latest
ON actor_metric_observations (source_kind, source_actor_id, observed_at DESC, created_at DESC);

CREATE INDEX idx_post_metric_observations_post_latest
ON post_metric_observations (source_kind, source_post_id, observed_at DESC, created_at DESC);
