-- ============================================================
-- Tweet V2 schema draft
-- PostgreSQL 17+ / 18 方言
-- ============================================================
--
-- 设计基线：
-- 1. 以 `/home/steven/code/x-monkey/src/schema/tweet-schema.ts` 的业务语义为来源。
-- 2. 存储思路以 `docs/claude3.sql` 为主，按写入模式拆分为 INSERT-dominant / UPSERT / Versioned。
-- 3. 已知且稳定的局部结构使用 PostgreSQL 复合类型与数组，不使用 JSONB；
--    `annotated_text` 仅保留在正文、长文、bio 与社区附注等核心富文本场景。
-- 4. 当前文件是结构草稿，目标是收敛数据库模型与写入策略，不直接作为正式 migration；
--    现阶段可视为冻结候选版本，除非出现明确结构性问题，否则不再继续扩展 DDL。
-- 5. 数据来源为爬虫：直接归属关系保留选择性外键；乱序、缺失、跨批次引用仅存 ID，不设外键。
-- 6. 字符串检索明确交由外部搜索引擎处理；数据库侧不再为全文检索、模糊匹配或 URL 搜索引入额外结构。
--
-- 写模式分类：
-- - INSERT-dominant：`twitter_user`、`tweet`、`media`、`hashtag`、`tweet_media_ref`、
--   `tweet_mention_ref`、`tweet_hashtag_ref`、`tweet_symbol_ref`、`string_dict`
-- - UPSERT：`user_category`、`symbol`、`tweet_place`、`tweet_edit`、`tweet_policy`、
--   `tweet_community_note`
-- - Versioned：`user_snapshot`、`user_stats`、`tweet_stats`、`media_resource`
--
-- 选择性外键策略：
-- - 保留物理外键：主实体到直接卫星表、关系表、字典表。
-- - 不设物理外键：`conversation_id`、`reply_to_tweet_id`、`quote_tweet_id`、`repost_id`、
--   `origin_tweet_id`、`origin_user_id`、`pinned_tweet_ids[]` 以及复合类型内部引用。
--
-- 命名字符串字典化范围：
-- - `tweet_text_style_name`
-- - `tweet_user_verification_type`
-- - `tweet_user_professional_type`
-- - `tweet_user_disclosure_relation`
-- - `tweet_user_parody_label`
-- - `tweet_user_avatar_shape`
-- - `tweet_country_name`
-- - `tweet_country_code`
-- - `tweet_language_code`
-- - `tweet_media_availability_status`
-- - `tweet_media_tag_kind`
-- - `tweet_media_resize_mode`
-- - `tweet_video_content_type`
-- - `tweet_place_kind`
-- - `tweet_reply_policy_code`
-- - `tweet_action_code`
-- - `tweet_source`

BEGIN;

-- ============================================================
-- 第一部分：枚举
-- ============================================================

-- 媒体类型枚举。
CREATE TYPE media_type_enum AS ENUM ('photo', 'video', 'animated_gif');

-- 命名字符串字典语义枚举。
CREATE TYPE string_semantic_enum AS ENUM (
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
    'tweet_media_tag_kind',
    'tweet_media_resize_mode',
    'tweet_video_content_type',
    'tweet_place_kind',
    'tweet_reply_policy_code',
    'tweet_action_code',
    'tweet_source'
);


-- ============================================================
-- 第二部分：复合类型
-- ============================================================

-- 通用坐标点，经度在前、纬度在后。
CREATE TYPE geo_point AS (
    longitude DOUBLE PRECISION,
    latitude DOUBLE PRECISION
);

-- 矩形区域，常用于焦点框。
CREATE TYPE media_rect AS (
    x INTEGER,
    y INTEGER,
    w INTEGER,
    h INTEGER
);

-- 已展开链接。
CREATE TYPE resolved_url AS (
    url TEXT,
    expanded_url TEXT,
    display_text TEXT
);

-- Hashtag 引用实体。
-- 实体文本统一提升到 hashtag 维表，正文内仅保留维表 ID 与文本区间；
-- 正向读取仍以 annotated_text 为主，不依赖维表 JOIN 回填正文文本。
CREATE TYPE hashtag_ref AS (
    hashtag_id INTEGER,
    range_start INTEGER,
    range_end INTEGER
);

-- 股票 / 符号引用实体。
-- 实体文本统一提升到 symbol 维表，正文内仅保留维表 ID 与文本区间；
-- 正向读取仍以内联正文为主，symbol 维表主要服务去重与反向查询。
CREATE TYPE symbol_ref AS (
    symbol_id INTEGER,
    range_start INTEGER,
    range_end INTEGER
);

-- URL 实体。
CREATE TYPE url_entity AS (
    url TEXT,
    expanded_url TEXT,
    display_text TEXT,
    range_start INTEGER,
    range_end INTEGER
);

-- @ 提及实体。
-- 不再冗余存储可变的 display name / user_name，只保留稳定 user_id 与文本区间。
CREATE TYPE mention_entity AS (
    user_id BIGINT,
    range_start INTEGER,
    range_end INTEGER
);

-- 文本中的媒体引用实体。
CREATE TYPE media_entity AS (
    media_id BIGINT,
    range_start INTEGER,
    range_end INTEGER,
    display_text TEXT,
    expanded_url TEXT,
    url TEXT,
    origin_tweet_id BIGINT,
    origin_user_id BIGINT
);

-- 富文本样式区间。
-- 样式名统一收敛为字典 ID 数组，semantic = tweet_text_style_name。
CREATE TYPE text_style_range AS (
    range_start INTEGER,
    range_end INTEGER,
    style_ids SMALLINT[]
);

-- 带实体信息的文本对象。
CREATE TYPE annotated_text AS (
    body TEXT,
    display_range_start INTEGER,
    display_range_end INTEGER,
    hashtags hashtag_ref[],
    symbols symbol_ref[],
    urls url_entity[],
    mentions mention_entity[],
    media_refs media_entity[],
    styles text_style_range[]
);

-- 用户认证信息。
CREATE TYPE user_verification AS (
    is_blue_verified BOOLEAN,
    verified_type_id SMALLINT
);

-- 用户账号披露关系。
-- 披露对象收敛为稳定标量字段，不再依赖 longDescription 等富文本明细。
CREATE TYPE user_disclosure AS (
    relation_id SMALLINT,
    subject_id BIGINT,
    subject_handle TEXT,
    subject_name TEXT,
    subject_url TEXT
);

-- 用户身份信息。
CREATE TYPE user_identity AS (
    verification user_verification,
    disclosure user_disclosure,
    parody_label_id SMALLINT,
    has_completed_new_account_review BOOLEAN,
    is_possibly_sensitive BOOLEAN
);

-- 用户能力与权限特征。
CREATE TYPE user_features AS (
    can_dm BOOLEAN,
    can_tag_media BOOLEAN,
    is_protected BOOLEAN,
    can_be_subscribed BOOLEAN
);

-- 专业账号信息。
-- 职业分类提升到 user_category 维表，快照中仅保留自增主键数组；
-- category_ids 由应用层维护，不对数组元素建立物理外键。
CREATE TYPE user_professional AS (
    professional_id BIGINT,
    professional_type_id SMALLINT,
    category_ids SMALLINT[]
);

-- 单个媒体尺寸版本。
CREATE TYPE media_size_variant AS (
    w INTEGER,
    h INTEGER,
    resize_mode_id SMALLINT
);

-- 按尺寸拆分的媒体版本集合。
CREATE TYPE media_size_variants AS (
    large media_size_variant,
    medium media_size_variant,
    small media_size_variant,
    thumb media_size_variant
);

-- 媒体几何信息。
CREATE TYPE media_geometry AS (
    w INTEGER,
    h INTEGER,
    focus_rects media_rect[]
);

-- 媒体中标记的用户。
-- 为降低重复与易变信息冗余，仅保留 user_id 与 kind_id。
CREATE TYPE media_tag AS (
    user_id BIGINT,
    kind_id SMALLINT
);

-- 媒体附加信息。
CREATE TYPE media_details AS (
    title TEXT,
    description TEXT,
    site_url TEXT,
    is_embeddable BOOLEAN,
    is_monetizable BOOLEAN
);

-- 视频流变体。
CREATE TYPE video_variant AS (
    content_type_id SMALLINT,
    bitrate INTEGER,
    url TEXT
);

-- 视频附加信息。
CREATE TYPE media_video AS (
    aspect_ratio_w INTEGER,
    aspect_ratio_h INTEGER,
    duration_ms BIGINT,
    variants video_variant[]
);

-- ============================================================
-- 第三部分：字典表
-- ============================================================

-- 命名字符串字典。
-- 统一采用共享字典表 + 语义枚举键；数据库层保证语义名只能取受控枚举值。
-- 该表只负责受控短字符串归一化，不承担任意字符串检索职责。
CREATE TABLE string_dict (
    id SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    semantic string_semantic_enum NOT NULL,
    value TEXT NOT NULL,
    CONSTRAINT uq_string_dict_semantic_value UNIQUE (semantic, value)
);

COMMENT ON TABLE string_dict IS '命名字符串字典表；仅存稳定且重复率高的短字符串；语义名由 string_semantic_enum 限定。建议服务端维护 (semantic, value) <-> id 本地缓存，在写入与读取侧避免为高频语义字符串反复查表或 JOIN。';
COMMENT ON COLUMN string_dict.semantic IS '字符串语义名，例如 tweet_language_code、tweet_reply_policy_code。';
COMMENT ON COLUMN string_dict.value IS '对应语义名下的原始字符串值。';

-- 获取或创建字典 ID 的辅助函数。
-- 该函数不是最终写入接口，只是便于草稿验证与后续实现时复用。
-- 约定：所有字典列均应通过显式 semantic 调用本函数或等价封装写入。
CREATE FUNCTION dict_id(p_semantic string_semantic_enum, p_value TEXT)
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
    FROM string_dict
    WHERE semantic = p_semantic
      AND value = p_value;

    IF FOUND THEN
        RETURN v_id;
    END IF;

    INSERT INTO string_dict (semantic, value)
    VALUES (p_semantic, p_value)
    ON CONFLICT (semantic, value) DO NOTHING
    RETURNING id INTO v_id;

    IF FOUND THEN
        RETURN v_id;
    END IF;

    SELECT id INTO v_id
    FROM string_dict
    WHERE semantic = p_semantic
      AND value = p_value;

    RETURN v_id;
END;
$$;

COMMENT ON FUNCTION dict_id(string_semantic_enum, TEXT) IS '获取或创建字典 ID；对空输入返回 NULL，使用并发安全的三步查找/插入流程；semantic 由调用方显式提供。建议服务端结合 string_dict 建立进程内缓存，减少重复查表与语义字符串反解成本。';


-- ============================================================
-- 第四部分：数据表
-- ============================================================

-- 用户主表。
-- 仅保留稳定主键与平台注册时间；可变资料全部进入快照表。
-- 该表以插入为主，但允许对稳定标量 `registered_at` 做非空回填。
CREATE TABLE twitter_user (
    id BIGINT PRIMARY KEY,
    registered_at TIMESTAMPTZ,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE twitter_user IS '用户主表；以插入为主，仅保存稳定身份与首次入库时间，允许对 registered_at 做非空回填。';
COMMENT ON COLUMN twitter_user.registered_at IS '平台侧注册时间，来自用户对象 createdAt；列可为空，后续允许用非空稳定值回填。';
COMMENT ON COLUMN twitter_user.ingested_at IS '本地首次入库时间。';

-- 用户全量快照表。
-- 合并 profile、identity、features、professional、pinned_tweet_ids。
CREATE TABLE user_snapshot (
    user_id BIGINT NOT NULL REFERENCES twitter_user(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,

    display_name TEXT NOT NULL,
    user_name TEXT NOT NULL,
    avatar_url TEXT,
    uses_default_avatar BOOLEAN,
    avatar_shape_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    banner_url TEXT,
    location TEXT,
    bio annotated_text,
    profile_links resolved_url[] NOT NULL DEFAULT ARRAY[]::resolved_url[],

    identity user_identity,
    features user_features,
    professional user_professional,
    pinned_tweet_ids BIGINT[] NOT NULL DEFAULT ARRAY[]::BIGINT[],

    CONSTRAINT pk_user_snapshot PRIMARY KEY (user_id, recorded_at)
);

COMMENT ON TABLE user_snapshot IS '用户全量快照表；版本化追加，应用层负责值变化去重。';
COMMENT ON COLUMN user_snapshot.avatar_shape_id IS '头像形状字典 ID，semantic = tweet_user_avatar_shape。';
COMMENT ON COLUMN user_snapshot.profile_links IS '资料页外链列表。';
COMMENT ON COLUMN user_snapshot.pinned_tweet_ids IS '置顶帖子 ID 数组，仅存储，不设数组级外键。';

-- 用户统计快照表。
CREATE TABLE user_stats (
    user_id BIGINT NOT NULL REFERENCES twitter_user(id) ON DELETE CASCADE,
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

COMMENT ON TABLE user_stats IS '用户统计快照表；版本化追加，与 user_snapshot 的采集节奏解耦。';

-- 用户职业分类维表。
-- 使用自增主键承载内部引用，保留来源分类编码作为稳定业务键。
CREATE TABLE user_category (
    id SMALLINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_category_code INTEGER NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_user_category_source_category_code UNIQUE (source_category_code)
);

COMMENT ON TABLE user_category IS '用户职业分类维表；按来源分类编码 UPSERT，供 user_professional.category_ids[] 引用。';
COMMENT ON COLUMN user_category.source_category_code IS '来源侧分类编码；非本地主键。';

-- Hashtag 维表。
-- 仅承担去重与引用，不作为数据库内全文检索入口。
CREATE TABLE hashtag (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tag TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_hashtag_tag UNIQUE (tag)
);

COMMENT ON TABLE hashtag IS 'Hashtag 维表；统一承载正文中的标签文本，供正文引用与反向查询使用。';
COMMENT ON COLUMN hashtag.tag IS '标签原文，不含前导 #。';

-- 股票 / 符号维表。
-- 仅承担去重与引用，不作为数据库内全文检索入口。
CREATE TABLE symbol (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    symbol TEXT NOT NULL,
    ticker TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_symbol_symbol UNIQUE (symbol)
);

COMMENT ON TABLE symbol IS '股票 / 符号维表；按 symbol UPSERT，供正文引用与反向查询使用。';
COMMENT ON COLUMN symbol.symbol IS '符号原文，通常不含前导 $。';

-- 地点维表。
-- 主键可以是上游 place.id，也可以是应用层为无 place.id 对象生成的稳定合成键。
CREATE TABLE tweet_place (
    id TEXT PRIMARY KEY,
    name TEXT,
    full_name TEXT,
    country_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    country_code_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    kind_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    boundary geo_point[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tweet_place IS '地点维表；单行 UPSERT 覆盖，可被多个帖子复用。';
COMMENT ON COLUMN tweet_place.country_id IS '国家名称字典 ID，semantic = tweet_country_name。';
COMMENT ON COLUMN tweet_place.country_code_id IS '国家代码字典 ID，semantic = tweet_country_code。';
COMMENT ON COLUMN tweet_place.kind_id IS '地点类型字典 ID，semantic = tweet_place_kind。';

-- 帖子主表。
-- 主表保留稳定主体、正文文本与会话 ID；可变状态拆到卫星表或快照表。
CREATE TABLE tweet (
    id BIGINT PRIMARY KEY,
    published_at TIMESTAMPTZ NOT NULL,
    source_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    author_id BIGINT NOT NULL REFERENCES twitter_user(id) ON DELETE RESTRICT,
    place_id TEXT REFERENCES tweet_place(id) ON DELETE RESTRICT,

    legacy_text annotated_text NOT NULL,
    note_id TEXT,
    note_text annotated_text,
    language_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,

    conversation_id BIGINT NOT NULL,
    reply_to_tweet_id BIGINT,
    reply_to_user_id BIGINT,
    quote_tweet_id BIGINT,
    quote_permalink resolved_url,
    repost_id BIGINT,

    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tweet IS '帖子主表；以插入为主，直接保存正文与会话主键字段。';
COMMENT ON COLUMN tweet.published_at IS '平台侧发布时间。';
COMMENT ON COLUMN tweet.source_id IS '来源渠道字典 ID，semantic = tweet_source。';
COMMENT ON COLUMN tweet.language_id IS '语言代码字典 ID，semantic = tweet_language_code。';
COMMENT ON COLUMN tweet.conversation_id IS '会话根帖 ID，不设外键以兼容乱序采集。';
COMMENT ON COLUMN tweet.reply_to_tweet_id IS '回复目标帖子 ID，不设外键以兼容目标缺失。';
COMMENT ON COLUMN tweet.quote_tweet_id IS '引用目标帖子 ID，不设外键以兼容目标缺失。';
COMMENT ON COLUMN tweet.repost_id IS '转贴原帖 ID，不设外键以兼容目标缺失。';

-- 帖子编辑信息。
CREATE TABLE tweet_edit (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet(id) ON DELETE CASCADE,
    version_ids BIGINT[] NOT NULL DEFAULT ARRAY[]::BIGINT[],
    editable_until TIMESTAMPTZ,
    remaining_edits INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ck_tweet_edit_remaining_edits_nonnegative CHECK (remaining_edits IS NULL OR remaining_edits >= 0)
);

COMMENT ON TABLE tweet_edit IS '帖子编辑信息表；按 tweet_id 单行 UPSERT。';
COMMENT ON COLUMN tweet_edit.version_ids IS '编辑版本帖子 ID 列表。';
COMMENT ON COLUMN tweet_edit.editable_until IS '可编辑截止时间，来源毫秒时间戳解析后存储。';

-- 帖子策略信息。
CREATE TABLE tweet_policy (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet(id) ON DELETE CASCADE,
    reply_policy_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    followers_only BOOLEAN,
    is_possibly_sensitive BOOLEAN,
    available_action_ids SMALLINT[] NOT NULL DEFAULT ARRAY[]::SMALLINT[],
    is_media_visibility_restricted BOOLEAN,
    paid_promotion BOOLEAN,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tweet_policy IS '帖子策略信息表；按 tweet_id 单行 UPSERT。';
COMMENT ON COLUMN tweet_policy.reply_policy_id IS '回复策略字典 ID，semantic = tweet_reply_policy_code。';
COMMENT ON COLUMN tweet_policy.available_action_ids IS '平台当前暴露的动作代码字典 ID 列表，semantic = tweet_action_code；数组级不设外键。';
COMMENT ON COLUMN tweet_policy.is_media_visibility_restricted IS '是否存在媒体可见性限制提示。';

-- 帖子交互统计快照表。
CREATE TABLE tweet_stats (
    tweet_id BIGINT NOT NULL REFERENCES tweet(id) ON DELETE CASCADE,
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

COMMENT ON TABLE tweet_stats IS '帖子统计快照表；版本化追加，主表不再保存最新版本引用。';

-- 社区附注表。
CREATE TABLE tweet_community_note (
    tweet_id BIGINT PRIMARY KEY REFERENCES tweet(id) ON DELETE CASCADE,
    note_id BIGINT,
    title TEXT,
    short_title TEXT,
    subtitle annotated_text,
    footer annotated_text,
    destination_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tweet_community_note IS '社区附注表；按 tweet_id 单行 UPSERT。';

-- 媒体主表。
-- 主表只存稳定元数据；URL、availability、video 放到版本表。
-- 当前标准化层优先采用 extended_entities.media，entities.media 仅用于补缺失 ID；
-- 因此本表仍按“以插入为主的稳定元数据表”建模，不因 review 中的弱对象假设改为 UPSERT。
CREATE TABLE media (
    id BIGINT PRIMARY KEY,
    type media_type_enum NOT NULL,
    alt_text TEXT,
    grok_post_id UUID,
    geometry media_geometry,
    size_variants media_size_variants,
    tagged_users media_tag[] NOT NULL DEFAULT ARRAY[]::media_tag[],
    origin_tweet_id BIGINT,
    origin_user_id BIGINT,
    details media_details,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE media IS '媒体主表；以插入为主，不保存 faces，也不内嵌 origin.user。';
COMMENT ON COLUMN media.grok_post_id IS 'Grok Post UUID；非雪花 ID。';
COMMENT ON COLUMN media.origin_tweet_id IS '来源帖子 ID，不设外键以兼容跨批次引用。';
COMMENT ON COLUMN media.origin_user_id IS '来源用户 ID，不设外键以兼容跨批次引用。';

-- 媒体资源快照表。
CREATE TABLE media_resource (
    media_id BIGINT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    recorded_at TIMESTAMPTZ NOT NULL,
    media_url TEXT,
    availability_id SMALLINT REFERENCES string_dict(id) ON DELETE RESTRICT,
    video media_video,

    CONSTRAINT pk_media_resource PRIMARY KEY (media_id, recorded_at)
);

COMMENT ON TABLE media_resource IS '媒体资源快照表；版本化追加，保存 URL、availability、video。';
COMMENT ON COLUMN media_resource.availability_id IS '媒体可用性字典 ID，semantic = tweet_media_availability_status。';

-- 帖子与媒体的关联表。
CREATE TABLE tweet_media_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet(id) ON DELETE CASCADE,
    media_id BIGINT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    display_order SMALLINT NOT NULL DEFAULT 0,

    CONSTRAINT pk_tweet_media_ref PRIMARY KEY (tweet_id, media_id),
    CONSTRAINT ck_tweet_media_ref_display_order_nonnegative CHECK (display_order >= 0)
);

COMMENT ON TABLE tweet_media_ref IS '帖子与媒体的关系表；替代 media_ids[]，支持双向查询与顺序恢复。';
COMMENT ON COLUMN tweet_media_ref.display_order IS '媒体在帖子中的展示顺序，从 0 开始。';

-- 帖子与被提及用户的关系表。
-- 作为正文 mention 实体的查询优化副本，用于按 user_id 反查帖子；
-- 正向读取仍以内联 annotated_text.mentions 为准。
CREATE TABLE tweet_mention_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,

    CONSTRAINT pk_tweet_mention_ref PRIMARY KEY (tweet_id, user_id)
);

COMMENT ON TABLE tweet_mention_ref IS '帖子与被提及用户的关系表；用于按 user_id 反查提及该用户的帖子。';
COMMENT ON COLUMN tweet_mention_ref.user_id IS '被提及用户 ID；不设物理外键，以兼容乱序采集与缺失用户。';

-- 帖子与 hashtag 的关系表。
-- 作为正文 hashtag 实体的查询优化副本；不用于正向恢复正文。
CREATE TABLE tweet_hashtag_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet(id) ON DELETE CASCADE,
    hashtag_id INTEGER NOT NULL REFERENCES hashtag(id) ON DELETE CASCADE,

    CONSTRAINT pk_tweet_hashtag_ref PRIMARY KEY (tweet_id, hashtag_id)
);

COMMENT ON TABLE tweet_hashtag_ref IS '帖子与 hashtag 的关系表；用于按 hashtag 反查帖子。';

-- 帖子与股票 / 符号的关系表。
-- 作为正文 symbol 实体的查询优化副本；不用于正向恢复正文。
CREATE TABLE tweet_symbol_ref (
    tweet_id BIGINT NOT NULL REFERENCES tweet(id) ON DELETE CASCADE,
    symbol_id INTEGER NOT NULL REFERENCES symbol(id) ON DELETE CASCADE,

    CONSTRAINT pk_tweet_symbol_ref PRIMARY KEY (tweet_id, symbol_id)
);

COMMENT ON TABLE tweet_symbol_ref IS '帖子与股票 / 符号的关系表；用于按 symbol 反查帖子。';


-- ============================================================
-- 第五部分：索引
-- ============================================================

-- 帖子按作者与发布时间的复合索引。
-- 可覆盖“按作者取最近帖子”的常见读取路径，且 author_id 仍可利用左前缀。
CREATE INDEX idx_tweet_author_published_at
ON tweet (author_id, published_at DESC);

-- 全局按发布时间检索的索引。
CREATE INDEX idx_tweet_published_at
ON tweet (published_at DESC);

-- 会话与关系索引。
CREATE INDEX idx_tweet_conversation
ON tweet (conversation_id);

CREATE INDEX idx_tweet_reply_to
ON tweet (reply_to_tweet_id)
WHERE reply_to_tweet_id IS NOT NULL;

CREATE INDEX idx_tweet_quote
ON tweet (quote_tweet_id)
WHERE quote_tweet_id IS NOT NULL;

CREATE INDEX idx_tweet_repost
ON tweet (repost_id)
WHERE repost_id IS NOT NULL;

-- 用户名检索与最新快照访问。
CREATE INDEX idx_user_snapshot_user_name_recorded_at
ON user_snapshot (user_name, recorded_at DESC);

-- 版本表“取最新值”索引。
CREATE INDEX idx_user_snapshot_latest
ON user_snapshot (user_id, recorded_at DESC);

CREATE INDEX idx_user_stats_latest
ON user_stats (user_id, recorded_at DESC);

CREATE INDEX idx_tweet_stats_latest
ON tweet_stats (tweet_id, recorded_at DESC);

CREATE INDEX idx_media_resource_latest
ON media_resource (media_id, recorded_at DESC);

-- 媒体来源反查索引。
CREATE INDEX idx_media_origin_tweet
ON media (origin_tweet_id)
WHERE origin_tweet_id IS NOT NULL;

CREATE INDEX idx_media_origin_user
ON media (origin_user_id)
WHERE origin_user_id IS NOT NULL;

-- 关系表索引：
-- 1. 反向按 media_id 查询关联帖子。
-- 2. 按 tweet_id 恢复媒体显示顺序。
CREATE INDEX idx_tweet_media_ref_media
ON tweet_media_ref (media_id);

CREATE INDEX idx_tweet_media_ref_tweet_order
ON tweet_media_ref (tweet_id, display_order, media_id);

CREATE INDEX idx_tweet_mention_ref_user
ON tweet_mention_ref (user_id, tweet_id);

CREATE INDEX idx_tweet_hashtag_ref_hashtag
ON tweet_hashtag_ref (hashtag_id, tweet_id);

CREATE INDEX idx_tweet_symbol_ref_symbol
ON tweet_symbol_ref (symbol_id, tweet_id);


-- ============================================================
-- 第六部分：存储参数
-- ============================================================

-- UPSERT 表预留页内空间，提升 HOT 更新命中率。
ALTER TABLE tweet_place SET (fillfactor = 85);
ALTER TABLE tweet_edit SET (fillfactor = 85);
ALTER TABLE tweet_policy SET (fillfactor = 85);
ALTER TABLE tweet_community_note SET (fillfactor = 85);

-- INSERT-only 大表放宽常规 vacuum / analyze 触发阈值。
ALTER TABLE tweet SET (
    autovacuum_vacuum_scale_factor = 0.10,
    autovacuum_analyze_scale_factor = 0.05
);

ALTER TABLE media SET (
    autovacuum_vacuum_scale_factor = 0.10,
    autovacuum_analyze_scale_factor = 0.05
);

-- UPSERT 表更积极地回收死元组。
ALTER TABLE tweet_place SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet_edit SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet_policy SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

ALTER TABLE tweet_community_note SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

-- 版本表主要是持续插入，优先关注 insert-driven vacuum。
ALTER TABLE user_snapshot SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE user_stats SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE tweet_stats SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);

ALTER TABLE media_resource SET (
    autovacuum_vacuum_insert_scale_factor = 0.05
);


-- ============================================================
-- 第七部分：便利视图
-- ============================================================

-- 用户最新全量快照。
CREATE VIEW v_latest_user_snapshot AS
SELECT DISTINCT ON (user_id) *
FROM user_snapshot
ORDER BY user_id, recorded_at DESC;

COMMENT ON VIEW v_latest_user_snapshot IS '每个用户的最新快照视图。';

-- 用户最新统计快照。
CREATE VIEW v_latest_user_stats AS
SELECT DISTINCT ON (user_id) *
FROM user_stats
ORDER BY user_id, recorded_at DESC;

COMMENT ON VIEW v_latest_user_stats IS '每个用户的最新统计视图。';

-- 帖子最新统计快照。
CREATE VIEW v_latest_tweet_stats AS
SELECT DISTINCT ON (tweet_id) *
FROM tweet_stats
ORDER BY tweet_id, recorded_at DESC;

COMMENT ON VIEW v_latest_tweet_stats IS '每条帖子的最新统计视图。';

-- 媒体最新资源快照。
CREATE VIEW v_latest_media_resource AS
SELECT DISTINCT ON (media_id) *
FROM media_resource
ORDER BY media_id, recorded_at DESC;

COMMENT ON VIEW v_latest_media_resource IS '每个媒体的最新资源视图。';

COMMIT;
