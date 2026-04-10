CREATE TYPE tweet.media_type_enum AS ENUM ('photo', 'video', 'animated_gif');

CREATE TYPE tweet.string_semantic_enum AS ENUM (
    'tweet_text_style_name',
    'tweet_user_verification_type',
    'tweet_user_professional_type',
    'tweet_user_disclosure_relation',
    'tweet_user_parody_label',
    'tweet_user_avatar_shape',
    'tweet_country_name',
    'tweet_country_code',
    'tweet_language_code',
    'tweet_media_availability_status',
    'tweet_media_sensitivity_code',
    'tweet_media_tag_kind',
    'tweet_media_resize_mode',
    'tweet_video_content_type',
    'tweet_place_kind',
    'tweet_reply_policy_code',
    'tweet_action_code',
    'tweet_source'
);

CREATE TYPE tweet.geo_point AS (
    longitude DOUBLE PRECISION,
    latitude DOUBLE PRECISION
);

CREATE TYPE tweet.media_rect AS (
    x INTEGER,
    y INTEGER,
    w INTEGER,
    h INTEGER
);

CREATE TYPE tweet.resolved_url AS (
    url TEXT,
    expanded_url TEXT,
    display_text TEXT
);

CREATE TYPE tweet.hashtag_ref AS (
    hashtag_id INTEGER,
    range_start INTEGER,
    range_end INTEGER
);

CREATE TYPE tweet.symbol_ref AS (
    symbol_id INTEGER,
    range_start INTEGER,
    range_end INTEGER
);

CREATE TYPE tweet.url_entity AS (
    url TEXT,
    expanded_url TEXT,
    display_text TEXT,
    range_start INTEGER,
    range_end INTEGER
);

CREATE TYPE tweet.mention_entity AS (
    user_id BIGINT,
    range_start INTEGER,
    range_end INTEGER
);

CREATE TYPE tweet.media_entity AS (
    media_id BIGINT,
    range_start INTEGER,
    range_end INTEGER,
    display_text TEXT,
    expanded_url TEXT,
    url TEXT,
    origin_tweet_id BIGINT,
    origin_user_id BIGINT
);

CREATE TYPE tweet.text_style_range AS (
    range_start INTEGER,
    range_end INTEGER,
    style_ids SMALLINT[]
);

CREATE TYPE tweet.annotated_text AS (
    body TEXT,
    display_range_start INTEGER,
    display_range_end INTEGER,
    hashtags tweet.hashtag_ref[],
    symbols tweet.symbol_ref[],
    urls tweet.url_entity[],
    mentions tweet.mention_entity[],
    media_refs tweet.media_entity[],
    styles tweet.text_style_range[]
);

CREATE TYPE tweet.user_verification AS (
    is_blue_verified BOOLEAN,
    verified_type_id SMALLINT
);

CREATE TYPE tweet.user_disclosure AS (
    relation_id SMALLINT,
    subject_id BIGINT,
    subject_handle TEXT,
    subject_name TEXT,
    subject_url TEXT
);

CREATE TYPE tweet.user_identity AS (
    verification tweet.user_verification,
    disclosure tweet.user_disclosure,
    parody_label_id SMALLINT,
    has_completed_new_account_review BOOLEAN,
    is_possibly_sensitive BOOLEAN
);

CREATE TYPE tweet.user_features AS (
    can_dm BOOLEAN,
    can_tag_media BOOLEAN,
    is_protected BOOLEAN,
    can_be_subscribed BOOLEAN
);

CREATE TYPE tweet.user_professional AS (
    professional_id BIGINT,
    professional_type_id SMALLINT,
    category_ids SMALLINT[]
);

CREATE TYPE tweet.media_size_variant AS (
    w INTEGER,
    h INTEGER,
    resize_mode_id SMALLINT
);

CREATE TYPE tweet.media_size_variants AS (
    large tweet.media_size_variant,
    medium tweet.media_size_variant,
    small tweet.media_size_variant,
    thumb tweet.media_size_variant
);

CREATE TYPE tweet.media_geometry AS (
    w INTEGER,
    h INTEGER,
    focus_rects tweet.media_rect[]
);

CREATE TYPE tweet.media_tag AS (
    user_id BIGINT,
    kind_id SMALLINT
);

CREATE TYPE tweet.media_details AS (
    title TEXT,
    description TEXT,
    site_url TEXT,
    is_embeddable BOOLEAN,
    is_monetizable BOOLEAN
);

CREATE TYPE tweet.video_variant AS (
    content_type_id SMALLINT,
    bitrate INTEGER,
    url TEXT
);

CREATE TYPE tweet.media_video AS (
    aspect_ratio_w INTEGER,
    aspect_ratio_h INTEGER,
    duration_ms BIGINT,
    variants tweet.video_variant[]
);

CREATE TABLE tweet.string_dict (
    id SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    semantic tweet.string_semantic_enum NOT NULL,
    value TEXT NOT NULL,
    CONSTRAINT uq_string_dict_semantic_value UNIQUE (semantic, value)
);

CREATE FUNCTION tweet.dict_id(p_semantic tweet.string_semantic_enum, p_value TEXT)
RETURNS SMALLINT
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_id SMALLINT;
BEGIN
    IF p_semantic IS NULL OR p_value IS NULL THEN
        RETURN NULL;
    END IF;

    SELECT id INTO v_id
    FROM tweet.string_dict
    WHERE semantic = p_semantic
      AND value = p_value;

    IF FOUND THEN
        RETURN v_id;
    END IF;

    INSERT INTO tweet.string_dict (semantic, value)
    VALUES (p_semantic, p_value)
    ON CONFLICT (semantic, value) DO NOTHING
    RETURNING id INTO v_id;

    IF FOUND THEN
        RETURN v_id;
    END IF;

    SELECT id INTO v_id
    FROM tweet.string_dict
    WHERE semantic = p_semantic
      AND value = p_value;

    RETURN v_id;
END;
$$;

CREATE TABLE tweet.twitter_user (
    id BIGINT PRIMARY KEY,
    registered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.user_snapshot (
    user_id BIGINT NOT NULL REFERENCES tweet.twitter_user(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,
    display_name TEXT NOT NULL,
    user_name TEXT NOT NULL,
    avatar_url TEXT,
    uses_default_avatar BOOLEAN,
    avatar_shape_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    banner_url TEXT,
    location TEXT,
    bio tweet.annotated_text,
    profile_links tweet.resolved_url[] NOT NULL DEFAULT ARRAY[]::tweet.resolved_url[],
    identity tweet.user_identity,
    features tweet.user_features,
    professional tweet.user_professional,
    pinned_tweet_ids BIGINT[] NOT NULL DEFAULT ARRAY[]::BIGINT[],
    CONSTRAINT pk_user_snapshot PRIMARY KEY (user_id, recorded_at)
);

CREATE TABLE tweet.user_stats (
    user_id BIGINT NOT NULL REFERENCES tweet.twitter_user(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,
    followers BIGINT,
    following BIGINT,
    likes BIGINT,
    media_posts BIGINT,
    tweets BIGINT,
    listed BIGINT,
    CONSTRAINT pk_user_stats PRIMARY KEY (user_id, recorded_at),
    CONSTRAINT ck_user_stats_followers_nonnegative CHECK (followers IS NULL OR followers >= 0),
    CONSTRAINT ck_user_stats_following_nonnegative CHECK (following IS NULL OR following >= 0),
    CONSTRAINT ck_user_stats_likes_nonnegative CHECK (likes IS NULL OR likes >= 0),
    CONSTRAINT ck_user_stats_media_posts_nonnegative CHECK (media_posts IS NULL OR media_posts >= 0),
    CONSTRAINT ck_user_stats_tweets_nonnegative CHECK (tweets IS NULL OR tweets >= 0),
    CONSTRAINT ck_user_stats_listed_nonnegative CHECK (listed IS NULL OR listed >= 0)
);

CREATE TABLE tweet.user_category (
    id SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_category_code INTEGER NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_user_category_source_category_code UNIQUE (source_category_code)
);

CREATE TABLE tweet.hashtag (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tag TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_hashtag_tag UNIQUE (tag)
);

CREATE TABLE tweet.symbol (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    symbol TEXT NOT NULL,
    ticker TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_symbol_symbol UNIQUE (symbol)
);

CREATE TABLE tweet.tweet_place (
    id TEXT PRIMARY KEY,
    name TEXT,
    full_name TEXT,
    country_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    country_code_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    kind_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    boundary tweet.geo_point[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.tweet (
    id BIGINT PRIMARY KEY,
    published_at TIMESTAMPTZ NOT NULL,
    source_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    author_id BIGINT NOT NULL REFERENCES tweet.twitter_user(id) ON DELETE RESTRICT,
    place_id TEXT REFERENCES tweet.tweet_place(id) ON DELETE RESTRICT,
    legacy_text tweet.annotated_text NOT NULL,
    note_id TEXT,
    note_text tweet.annotated_text,
    language_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    conversation_id BIGINT NOT NULL,
    reply_to_tweet_id BIGINT,
    reply_to_user_id BIGINT,
    quote_tweet_id BIGINT,
    quote_permalink tweet.resolved_url,
    repost_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.tweet_edit (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    version_ids BIGINT[] NOT NULL DEFAULT ARRAY[]::BIGINT[],
    editable_until TIMESTAMPTZ,
    remaining_edits INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT ck_tweet_edit_remaining_edits_nonnegative CHECK (remaining_edits IS NULL OR remaining_edits >= 0)
);

CREATE TABLE tweet.tweet_policy (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    reply_policy_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    followers_only BOOLEAN,
    is_possibly_sensitive BOOLEAN,
    available_action_ids SMALLINT[] NOT NULL DEFAULT ARRAY[]::SMALLINT[],
    is_media_visibility_restricted BOOLEAN,
    paid_promotion BOOLEAN,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.tweet_stats (
    tweet_id BIGINT NOT NULL REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,
    views BIGINT,
    replies BIGINT,
    reposts BIGINT,
    quotes BIGINT,
    likes BIGINT,
    bookmarks BIGINT,
    CONSTRAINT pk_tweet_stats PRIMARY KEY (tweet_id, recorded_at),
    CONSTRAINT ck_tweet_stats_views_nonnegative CHECK (views IS NULL OR views >= 0),
    CONSTRAINT ck_tweet_stats_replies_nonnegative CHECK (replies IS NULL OR replies >= 0),
    CONSTRAINT ck_tweet_stats_reposts_nonnegative CHECK (reposts IS NULL OR reposts >= 0),
    CONSTRAINT ck_tweet_stats_quotes_nonnegative CHECK (quotes IS NULL OR quotes >= 0),
    CONSTRAINT ck_tweet_stats_likes_nonnegative CHECK (likes IS NULL OR likes >= 0),
    CONSTRAINT ck_tweet_stats_bookmarks_nonnegative CHECK (bookmarks IS NULL OR bookmarks >= 0)
);

CREATE TABLE tweet.tweet_community_note (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    note_id BIGINT,
    title TEXT,
    short_title TEXT,
    subtitle tweet.annotated_text,
    footer tweet.annotated_text,
    destination_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.media (
    id BIGINT PRIMARY KEY,
    type tweet.media_type_enum NOT NULL,
    alt_text TEXT,
    grok_post_id UUID,
    geometry tweet.media_geometry,
    size_variants tweet.media_size_variants,
    tagged_users tweet.media_tag[] NOT NULL DEFAULT ARRAY[]::tweet.media_tag[],
    sensitivity_warning_ids SMALLINT[] NOT NULL DEFAULT ARRAY[]::SMALLINT[],
    origin_tweet_id BIGINT,
    origin_user_id BIGINT,
    details tweet.media_details,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tweet.media_resource (
    media_id BIGINT NOT NULL REFERENCES tweet.media(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,
    media_url TEXT,
    availability_id SMALLINT REFERENCES tweet.string_dict(id) ON DELETE RESTRICT,
    video tweet.media_video,
    CONSTRAINT pk_media_resource PRIMARY KEY (media_id, recorded_at)
);

CREATE TABLE tweet.tweet_media_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    media_id BIGINT NOT NULL REFERENCES tweet.media(id) ON DELETE CASCADE,
    display_order SMALLINT NOT NULL DEFAULT 0,
    CONSTRAINT pk_tweet_media_ref PRIMARY KEY (tweet_id, media_id),
    CONSTRAINT ck_tweet_media_ref_display_order_nonnegative CHECK (display_order >= 0)
);

CREATE TABLE tweet.tweet_mention_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    CONSTRAINT pk_tweet_mention_ref PRIMARY KEY (tweet_id, user_id)
);

CREATE TABLE tweet.tweet_hashtag_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    hashtag_id INTEGER NOT NULL REFERENCES tweet.hashtag(id) ON DELETE CASCADE,
    CONSTRAINT pk_tweet_hashtag_ref PRIMARY KEY (tweet_id, hashtag_id)
);

CREATE TABLE tweet.tweet_symbol_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet.tweet(id) ON DELETE CASCADE,
    symbol_id INTEGER NOT NULL REFERENCES tweet.symbol(id) ON DELETE CASCADE,
    CONSTRAINT pk_tweet_symbol_ref PRIMARY KEY (tweet_id, symbol_id)
);

CREATE INDEX idx_tweet_author_published_at
ON tweet.tweet (author_id, published_at DESC);

CREATE INDEX idx_tweet_published_at
ON tweet.tweet (published_at DESC);

CREATE INDEX idx_tweet_conversation
ON tweet.tweet (conversation_id);

CREATE INDEX idx_tweet_reply_to
ON tweet.tweet (reply_to_tweet_id)
WHERE reply_to_tweet_id IS NOT NULL;

CREATE INDEX idx_tweet_quote
ON tweet.tweet (quote_tweet_id)
WHERE quote_tweet_id IS NOT NULL;

CREATE INDEX idx_tweet_repost
ON tweet.tweet (repost_id)
WHERE repost_id IS NOT NULL;

CREATE INDEX idx_user_snapshot_user_name_recorded_at
ON tweet.user_snapshot (user_name, recorded_at DESC);

CREATE INDEX idx_user_snapshot_latest
ON tweet.user_snapshot (user_id, recorded_at DESC);

CREATE INDEX idx_user_stats_latest
ON tweet.user_stats (user_id, recorded_at DESC);

CREATE INDEX idx_tweet_stats_latest
ON tweet.tweet_stats (tweet_id, recorded_at DESC);

CREATE INDEX idx_media_resource_latest
ON tweet.media_resource (media_id, recorded_at DESC);

CREATE INDEX idx_media_origin_tweet
ON tweet.media (origin_tweet_id)
WHERE origin_tweet_id IS NOT NULL;

CREATE INDEX idx_media_origin_user
ON tweet.media (origin_user_id)
WHERE origin_user_id IS NOT NULL;

CREATE INDEX idx_tweet_media_ref_media
ON tweet.tweet_media_ref (media_id);

CREATE INDEX idx_tweet_media_ref_tweet_order
ON tweet.tweet_media_ref (tweet_id, display_order, media_id);

CREATE INDEX idx_tweet_mention_ref_user
ON tweet.tweet_mention_ref (user_id, tweet_id);

CREATE INDEX idx_tweet_hashtag_ref_hashtag
ON tweet.tweet_hashtag_ref (hashtag_id, tweet_id);

CREATE INDEX idx_tweet_symbol_ref_symbol
ON tweet.tweet_symbol_ref (symbol_id, tweet_id);

ALTER TABLE tweet.tweet_place SET (fillfactor = 85);
ALTER TABLE tweet.tweet_edit SET (fillfactor = 85);
ALTER TABLE tweet.tweet_policy SET (fillfactor = 85);
ALTER TABLE tweet.tweet_community_note SET (fillfactor = 85);

ALTER TABLE tweet.tweet SET (
    autovacuum_vacuum_scale_factor = 0.10,
    autovacuum_analyze_scale_factor = 0.05
);

ALTER TABLE tweet.media SET (
    autovacuum_vacuum_scale_factor = 0.10,
    autovacuum_analyze_scale_factor = 0.05
);

ALTER TABLE tweet.tweet_place SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet.tweet_edit SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet.tweet_policy SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet.tweet_community_note SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet.user_snapshot SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE tweet.user_stats SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE tweet.tweet_stats SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE tweet.media_resource SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

CREATE VIEW tweet.v_latest_user_snapshot AS
SELECT DISTINCT ON (user_id) *
FROM tweet.user_snapshot
ORDER BY user_id, recorded_at DESC;

CREATE VIEW tweet.v_latest_user_stats AS
SELECT DISTINCT ON (user_id) *
FROM tweet.user_stats
ORDER BY user_id, recorded_at DESC;

CREATE VIEW tweet.v_latest_tweet_stats AS
SELECT DISTINCT ON (tweet_id) *
FROM tweet.tweet_stats
ORDER BY tweet_id, recorded_at DESC;

CREATE VIEW tweet.v_latest_media_resource AS
SELECT DISTINCT ON (media_id) *
FROM tweet.media_resource
ORDER BY media_id, recorded_at DESC;
