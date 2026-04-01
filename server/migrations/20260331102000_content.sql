CREATE TABLE ingest_submissions (
    id UUID PRIMARY KEY,
    submitter_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    source_kind TEXT NOT NULL,
    users_count INTEGER NOT NULL DEFAULT 0,
    tweets_count INTEGER NOT NULL DEFAULT 0,
    media_count INTEGER NOT NULL DEFAULT 0,
    accepted_count INTEGER NOT NULL DEFAULT 0,
    warnings JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL CHECK (status IN ('processing', 'success', 'partial', 'failed')),
    transfer_jobs_enqueued INTEGER NOT NULL DEFAULT 0,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

CREATE INDEX idx_ingest_submissions_received_at
ON ingest_submissions (received_at DESC);

CREATE TABLE managed_media (
    id UUID PRIMARY KEY,
    source_kind TEXT NOT NULL,
    media_family TEXT NOT NULL CHECK (media_family IN ('image', 'video', 'animated_gif')),
    identity_kind TEXT NOT NULL CHECK (identity_kind IN ('post_source_url', 'actor_avatar_url', 'actor_banner_url')),
    identity_value TEXT NOT NULL,
    fetch_url TEXT NOT NULL,
    display_url TEXT NOT NULL DEFAULT '',
    thumb_url TEXT,
    content_type_hint TEXT NOT NULL DEFAULT '',
    first_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    last_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, identity_kind, identity_value)
);

CREATE INDEX idx_managed_media_last_observed_at
ON managed_media (source_kind, last_observed_at DESC);

CREATE TABLE actors (
    source_kind TEXT NOT NULL,
    source_actor_id TEXT NOT NULL,
    current_profile_version_id UUID,
    first_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    last_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (source_kind, source_actor_id)
);

CREATE INDEX idx_actors_last_observed_at
ON actors (source_kind, last_observed_at DESC);

CREATE TABLE actor_profile_versions (
    id UUID PRIMARY KEY,
    submission_id UUID NOT NULL REFERENCES ingest_submissions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_actor_id TEXT NOT NULL,
    version_no BIGINT NOT NULL,
    effective_from TIMESTAMPTZ NOT NULL,
    effective_to TIMESTAMPTZ,
    profile_fingerprint TEXT NOT NULL,
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
    pinned_post_source_ids TEXT[] NOT NULL DEFAULT '{}',
    source_created_at_raw TEXT NOT NULL DEFAULT '',
    avatar_media_id UUID REFERENCES managed_media(id) ON DELETE SET NULL,
    banner_media_id UUID REFERENCES managed_media(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_kind, source_actor_id, version_no),
    FOREIGN KEY (source_kind, source_actor_id)
        REFERENCES actors(source_kind, source_actor_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_actor_profile_versions_actor_id
ON actor_profile_versions (source_kind, source_actor_id, effective_from DESC);

CREATE INDEX idx_actor_profile_versions_current
ON actor_profile_versions (source_kind, source_actor_id)
WHERE effective_to IS NULL;

CREATE TABLE actor_metric_observations (
    submission_id UUID NOT NULL REFERENCES ingest_submissions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_actor_id TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    followers_count BIGINT NOT NULL DEFAULT 0,
    friends_count BIGINT NOT NULL DEFAULT 0,
    favourites_count BIGINT NOT NULL DEFAULT 0,
    statuses_count BIGINT NOT NULL DEFAULT 0,
    media_count BIGINT NOT NULL DEFAULT 0,
    listed_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (submission_id, source_kind, source_actor_id),
    FOREIGN KEY (source_kind, source_actor_id)
        REFERENCES actors(source_kind, source_actor_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_actor_metric_observations_actor_id
ON actor_metric_observations (source_kind, source_actor_id, observed_at DESC);

CREATE TABLE posts (
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
    possibly_sensitive BOOLEAN,
    source_label TEXT NOT NULL DEFAULT '',
    first_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    last_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (source_kind, source_post_id)
);

CREATE INDEX idx_posts_last_observed_at
ON posts (source_kind, last_observed_at DESC);

CREATE INDEX idx_posts_author_source_actor_id
ON posts (source_kind, author_source_actor_id);

CREATE TABLE post_media (
    source_kind TEXT NOT NULL,
    source_post_id TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (source_kind, source_post_id, source_media_id),
    FOREIGN KEY (source_kind, source_post_id)
        REFERENCES posts(source_kind, source_post_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_post_media_post_id
ON post_media (source_kind, source_post_id, position ASC);

CREATE TABLE post_metric_observations (
    submission_id UUID NOT NULL REFERENCES ingest_submissions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_post_id TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    view_count BIGINT,
    favorite_count BIGINT NOT NULL DEFAULT 0,
    retweet_count BIGINT NOT NULL DEFAULT 0,
    reply_count BIGINT NOT NULL DEFAULT 0,
    quote_count BIGINT NOT NULL DEFAULT 0,
    bookmark_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (submission_id, source_kind, source_post_id),
    FOREIGN KEY (source_kind, source_post_id)
        REFERENCES posts(source_kind, source_post_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_post_metric_observations_post_id
ON post_metric_observations (source_kind, source_post_id, observed_at DESC);

CREATE TABLE post_media_sources (
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    managed_media_id UUID NOT NULL REFERENCES managed_media(id) ON DELETE RESTRICT,
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
    first_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    last_submission_id UUID REFERENCES ingest_submissions(id) ON DELETE SET NULL,
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (source_kind, source_media_id)
);

CREATE INDEX idx_post_media_sources_post_id
ON post_media_sources (source_kind, source_post_id);

CREATE INDEX idx_post_media_sources_last_observed_at
ON post_media_sources (source_kind, last_observed_at DESC);

CREATE TABLE media_variants (
    source_kind TEXT NOT NULL,
    source_media_id TEXT NOT NULL,
    url TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    bitrate BIGINT,
    content_type TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (source_kind, source_media_id, url),
    FOREIGN KEY (source_kind, source_media_id)
        REFERENCES post_media_sources(source_kind, source_media_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_media_variants_media_id
ON media_variants (source_kind, source_media_id, position ASC);
