CREATE TABLE ingest_submissions (
    id UUID PRIMARY KEY,
    submitter_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    source_kind TEXT NOT NULL,
    client_context JSONB NOT NULL DEFAULT '{}'::jsonb,
    request_body JSONB NOT NULL,
    users_count INTEGER NOT NULL DEFAULT 0,
    tweets_count INTEGER NOT NULL DEFAULT 0,
    media_count INTEGER NOT NULL DEFAULT 0,
    captures_count INTEGER NOT NULL DEFAULT 0,
    timeline_events_count INTEGER NOT NULL DEFAULT 0,
    accepted_count INTEGER NOT NULL DEFAULT 0,
    warnings JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL CHECK (status IN ('processing', 'success', 'partial', 'failed')),
    transfer_jobs_enqueued INTEGER NOT NULL DEFAULT 0,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

CREATE INDEX idx_ingest_submissions_received_at
ON ingest_submissions (received_at DESC);

CREATE TABLE actors (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_actor_id TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    screen_name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    avatar_url TEXT NOT NULL DEFAULT '',
    profile_url TEXT,
    banner_url TEXT,
    is_blue_verified BOOLEAN NOT NULL DEFAULT FALSE,
    verified_type TEXT,
    is_protected BOOLEAN NOT NULL DEFAULT FALSE,
    profile_image_shape TEXT NOT NULL DEFAULT '',
    professional_type TEXT,
    followers_count BIGINT NOT NULL DEFAULT 0,
    friends_count BIGINT NOT NULL DEFAULT 0,
    favourites_count BIGINT NOT NULL DEFAULT 0,
    statuses_count BIGINT NOT NULL DEFAULT 0,
    media_count BIGINT NOT NULL DEFAULT 0,
    listed_count BIGINT NOT NULL DEFAULT 0,
    pinned_post_source_ids TEXT[] NOT NULL DEFAULT '{}',
    source_created_at_raw TEXT NOT NULL DEFAULT '',
    first_submission_id UUID,
    last_submission_id UUID,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_actor_id)
);

CREATE INDEX idx_actors_last_observed_at
ON actors (last_observed_at DESC);

CREATE TABLE posts (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_post_id TEXT NOT NULL,
    author_source_actor_id TEXT NOT NULL DEFAULT '',
    conversation_source_post_id TEXT NOT NULL DEFAULT '',
    full_text TEXT NOT NULL DEFAULT '',
    legacy_full_text TEXT NOT NULL DEFAULT '',
    note_text TEXT,
    lang TEXT NOT NULL DEFAULT '',
    source_created_at_raw TEXT NOT NULL DEFAULT '',
    in_reply_to_source_post_id TEXT,
    in_reply_to_source_actor_id TEXT,
    quoted_source_post_id TEXT,
    retweeted_source_post_id TEXT,
    view_count BIGINT,
    possibly_sensitive BOOLEAN,
    favorite_count BIGINT NOT NULL DEFAULT 0,
    retweet_count BIGINT NOT NULL DEFAULT 0,
    reply_count BIGINT NOT NULL DEFAULT 0,
    quote_count BIGINT NOT NULL DEFAULT 0,
    bookmark_count BIGINT NOT NULL DEFAULT 0,
    media_source_ids TEXT[] NOT NULL DEFAULT '{}',
    source_label TEXT NOT NULL DEFAULT '',
    first_submission_id UUID,
    last_submission_id UUID,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_post_id)
);

CREATE INDEX idx_posts_last_observed_at
ON posts (last_observed_at DESC);

CREATE INDEX idx_posts_author_source_actor_id
ON posts (source_kind, author_source_actor_id);

CREATE INDEX idx_posts_media_source_ids_gin
ON posts USING GIN (media_source_ids);

CREATE TABLE media (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    media_key TEXT NOT NULL DEFAULT '',
    source_post_id TEXT NOT NULL DEFAULT '',
    media_type TEXT NOT NULL DEFAULT '',
    media_url TEXT NOT NULL DEFAULT '',
    thumb_url TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL DEFAULT '',
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    alt_text TEXT,
    allow_download BOOLEAN NOT NULL DEFAULT FALSE,
    source_status_id TEXT,
    source_actor_id TEXT,
    duration_ms BIGINT,
    first_submission_id UUID,
    last_submission_id UUID,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_media_id)
);

CREATE INDEX idx_media_source_post_id
ON media (source_kind, source_post_id);

CREATE TABLE capture_events (
    id UUID PRIMARY KEY,
    submission_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    capture_id TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    method TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    graphql_id TEXT NOT NULL DEFAULT '',
    operation_name TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 0,
    status_text TEXT NOT NULL DEFAULT '',
    response_headers TEXT NOT NULL DEFAULT '',
    response_body TEXT NOT NULL DEFAULT '',
    response_size BIGINT NOT NULL DEFAULT 0,
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_capture_events_submission_id
ON capture_events (submission_id, captured_at DESC);

CREATE TABLE post_metric_observations (
    id UUID PRIMARY KEY,
    submission_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    source_post_id TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    view_count BIGINT,
    favorite_count BIGINT NOT NULL DEFAULT 0,
    retweet_count BIGINT NOT NULL DEFAULT 0,
    reply_count BIGINT NOT NULL DEFAULT 0,
    quote_count BIGINT NOT NULL DEFAULT 0,
    bookmark_count BIGINT NOT NULL DEFAULT 0,
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_post_metric_observations_post_id
ON post_metric_observations (source_kind, source_post_id, observed_at DESC);

CREATE TABLE timeline_observations (
    id UUID PRIMARY KEY,
    submission_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    timeline_kind TEXT NOT NULL,
    timeline_key TEXT NOT NULL,
    post_source_ids TEXT[] NOT NULL DEFAULT '{}',
    observed_at TIMESTAMPTZ NOT NULL,
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_timeline_observations_timeline
ON timeline_observations (source_kind, timeline_kind, timeline_key, observed_at DESC);

CREATE INDEX idx_timeline_observations_post_source_ids_gin
ON timeline_observations USING GIN (post_source_ids);

CREATE TABLE media_variant_observations (
    id UUID PRIMARY KEY,
    submission_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    bitrate BIGINT,
    content_type TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    observed_at TIMESTAMPTZ NOT NULL,
    raw_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_media_variant_observations_media_id
ON media_variant_observations (source_kind, source_media_id, observed_at DESC);
