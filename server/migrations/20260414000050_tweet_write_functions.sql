CREATE OR REPLACE FUNCTION tweet.resolve_user_categories(p_categories JSONB)
RETURNS TABLE(source_category_code INTEGER, id SMALLINT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.source_category_code)
            item.source_category_code,
            item.name
        FROM jsonb_to_recordset(p_categories) AS item(
            source_category_code INTEGER,
            name TEXT
        )
        ORDER BY item.source_category_code, btrim(COALESCE(item.name, '')) = ''
    ),
    upserted AS (
        INSERT INTO tweet.user_category (source_category_code, name)
        SELECT input.source_category_code, input.name
        FROM input
        ON CONFLICT (source_category_code) DO UPDATE
        SET name = EXCLUDED.name,
            updated_at = NOW()
        WHERE tweet.user_category.name = ''
          AND EXCLUDED.name <> ''
        RETURNING source_category_code, id
    )
    SELECT upserted.source_category_code, upserted.id
    FROM upserted
    UNION
    SELECT existing.source_category_code, existing.id
    FROM tweet.user_category AS existing
    JOIN input
      ON input.source_category_code = existing.source_category_code;
$$;

CREATE OR REPLACE FUNCTION tweet.resolve_hashtags(p_hashtags JSONB)
RETURNS TABLE(tag TEXT, id INTEGER)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.tag)
            item.tag
        FROM jsonb_to_recordset(p_hashtags) AS item(tag TEXT)
        ORDER BY item.tag
    ),
    inserted AS (
        INSERT INTO tweet.hashtag (tag)
        SELECT input.tag
        FROM input
        ON CONFLICT (tag) DO NOTHING
        RETURNING tag, id
    )
    SELECT inserted.tag, inserted.id
    FROM inserted
    UNION
    SELECT existing.tag, existing.id
    FROM tweet.hashtag AS existing
    JOIN input
      ON input.tag = existing.tag;
$$;

CREATE OR REPLACE FUNCTION tweet.resolve_symbols(p_symbols JSONB)
RETURNS TABLE(symbol TEXT, id INTEGER)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.symbol)
            item.symbol,
            item.ticker,
            item.name
        FROM jsonb_to_recordset(p_symbols) AS item(
            symbol TEXT,
            ticker TEXT,
            name TEXT
        )
        ORDER BY item.symbol, item.ticker IS NULL, item.name IS NULL
    ),
    upserted AS (
        INSERT INTO tweet.symbol (symbol, ticker, name)
        SELECT input.symbol, input.ticker, input.name
        FROM input
        ON CONFLICT (symbol) DO UPDATE
        SET ticker = COALESCE(tweet.symbol.ticker, EXCLUDED.ticker),
            name = COALESCE(tweet.symbol.name, EXCLUDED.name),
            updated_at = NOW()
        WHERE (tweet.symbol.ticker IS NULL AND EXCLUDED.ticker IS NOT NULL)
           OR (tweet.symbol.name IS NULL AND EXCLUDED.name IS NOT NULL)
        RETURNING symbol, id
    )
    SELECT upserted.symbol, upserted.id
    FROM upserted
    UNION
    SELECT existing.symbol, existing.id
    FROM tweet.symbol AS existing
    JOIN input
      ON input.symbol = existing.symbol;
$$;

CREATE OR REPLACE FUNCTION tweet.write_twitter_users(p_users JSONB)
RETURNS TABLE(id BIGINT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.id)
            item.id,
            item.registered_at
        FROM jsonb_to_recordset(p_users) AS item(
            id BIGINT,
            registered_at TIMESTAMPTZ
        )
        ORDER BY item.id, item.registered_at IS NULL
    ),
    changed AS (
        INSERT INTO tweet.twitter_user (id, registered_at)
        SELECT input.id, input.registered_at
        FROM input
        ON CONFLICT (id) DO UPDATE
        SET registered_at = EXCLUDED.registered_at,
            updated_at = NOW()
        WHERE tweet.twitter_user.registered_at IS NULL
          AND EXCLUDED.registered_at IS NOT NULL
        RETURNING id
    )
    SELECT changed.id
    FROM changed;
$$;

CREATE OR REPLACE FUNCTION tweet.append_user_snapshots_if_changed(p_snapshots JSONB)
RETURNS TABLE(user_id BIGINT, recorded_at TIMESTAMPTZ, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.user_id, item.recorded_at)
            item.user_id,
            item.recorded_at,
            item.display_name,
            item.user_name,
            item.avatar_url,
            item.uses_default_avatar,
            item.avatar_shape_id,
            item.banner_url,
            item.location,
            item.bio,
            COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]) AS profile_links,
            item.identity,
            item.features,
            item.professional,
            COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[]) AS pinned_tweet_ids
        FROM jsonb_to_recordset(p_snapshots) AS item(
            user_id BIGINT,
            recorded_at TIMESTAMPTZ,
            display_name TEXT,
            user_name TEXT,
            avatar_url TEXT,
            uses_default_avatar BOOLEAN,
            avatar_shape_id SMALLINT,
            banner_url TEXT,
            location TEXT,
            bio tweet.annotated_text,
            profile_links tweet.resolved_url[],
            identity tweet.user_identity,
            features tweet.user_features,
            professional tweet.user_professional,
            pinned_tweet_ids BIGINT[]
        )
        ORDER BY item.user_id, item.recorded_at
    ),
    existing_parent AS (
        SELECT input.user_id, input.recorded_at
        FROM input
        JOIN tweet.twitter_user AS parent
          ON parent.id = input.user_id
    ),
    classified AS (
        SELECT
            input.*,
            (existing_parent.user_id IS NOT NULL) AS has_parent,
            (
                existing_parent.user_id IS NOT NULL
            AND latest.user_id IS NOT NULL
            AND latest.display_name IS NOT DISTINCT FROM input.display_name
            AND latest.user_name IS NOT DISTINCT FROM input.user_name
            AND latest.avatar_url IS NOT DISTINCT FROM input.avatar_url
            AND latest.uses_default_avatar IS NOT DISTINCT FROM input.uses_default_avatar
            AND latest.avatar_shape_id IS NOT DISTINCT FROM input.avatar_shape_id
            AND latest.banner_url IS NOT DISTINCT FROM input.banner_url
            AND latest.location IS NOT DISTINCT FROM input.location
            AND to_jsonb(latest.bio) IS NOT DISTINCT FROM to_jsonb(input.bio)
            AND to_jsonb(latest.profile_links) IS NOT DISTINCT FROM to_jsonb(input.profile_links)
            AND to_jsonb(latest.identity) IS NOT DISTINCT FROM to_jsonb(input.identity)
            AND to_jsonb(latest.features) IS NOT DISTINCT FROM to_jsonb(input.features)
            AND to_jsonb(latest.professional) IS NOT DISTINCT FROM to_jsonb(input.professional)
            AND latest.pinned_tweet_ids IS NOT DISTINCT FROM input.pinned_tweet_ids
            ) AS unchanged
        FROM input
        LEFT JOIN existing_parent
          ON existing_parent.user_id = input.user_id
         AND existing_parent.recorded_at = input.recorded_at
        LEFT JOIN LATERAL (
            SELECT *
            FROM tweet.user_snapshot AS latest
            WHERE latest.user_id = input.user_id
              AND latest.recorded_at <= input.recorded_at
            ORDER BY latest.recorded_at DESC
            LIMIT 1
        ) AS latest ON true
    ),
    inserted AS (
        INSERT INTO tweet.user_snapshot (
            user_id,
            recorded_at,
            display_name,
            user_name,
            avatar_url,
            uses_default_avatar,
            avatar_shape_id,
            banner_url,
            location,
            bio,
            profile_links,
            identity,
            features,
            professional,
            pinned_tweet_ids
        )
        SELECT
            user_id,
            recorded_at,
            display_name,
            user_name,
            avatar_url,
            uses_default_avatar,
            avatar_shape_id,
            banner_url,
            location,
            bio,
            profile_links,
            identity,
            features,
            professional,
            pinned_tweet_ids
        FROM classified
        WHERE has_parent
          AND NOT unchanged
        ON CONFLICT (user_id, recorded_at) DO NOTHING
        RETURNING user_id, recorded_at
    )
    SELECT
        classified.user_id,
        classified.recorded_at,
        CASE
            WHEN NOT classified.has_parent THEN 'missing_parent'
            WHEN inserted.user_id IS NOT NULL THEN 'inserted'
            WHEN classified.unchanged THEN 'unchanged'
            ELSE 'duplicate'
        END AS status
    FROM classified
    LEFT JOIN inserted
      ON inserted.user_id = classified.user_id
     AND inserted.recorded_at = classified.recorded_at;
$$;

CREATE OR REPLACE FUNCTION tweet.append_user_stats_if_changed(p_stats JSONB, p_min_interval_seconds BIGINT)
RETURNS TABLE(user_id BIGINT, recorded_at TIMESTAMPTZ, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.user_id, item.recorded_at)
            item.user_id,
            item.recorded_at,
            item.followers,
            item.following,
            item.likes,
            item.media_posts,
            item.tweets,
            item.listed
        FROM jsonb_to_recordset(p_stats) AS item(
            user_id BIGINT,
            recorded_at TIMESTAMPTZ,
            followers BIGINT,
            following BIGINT,
            likes BIGINT,
            media_posts BIGINT,
            tweets BIGINT,
            listed BIGINT
        )
        ORDER BY item.user_id, item.recorded_at
    ),
    existing_parent AS (
        SELECT input.user_id, input.recorded_at
        FROM input
        JOIN tweet.twitter_user AS parent
          ON parent.id = input.user_id
    ),
    classified AS (
        SELECT
            input.*,
            CASE
                WHEN existing_parent.user_id IS NULL THEN 'missing_parent'
                WHEN latest.recorded_at IS NOT NULL
                 AND latest.followers IS NOT DISTINCT FROM input.followers
                 AND latest.following IS NOT DISTINCT FROM input.following
                 AND latest.likes IS NOT DISTINCT FROM input.likes
                 AND latest.media_posts IS NOT DISTINCT FROM input.media_posts
                 AND latest.tweets IS NOT DISTINCT FROM input.tweets
                 AND latest.listed IS NOT DISTINCT FROM input.listed
                    THEN 'unchanged'
                WHEN latest.recorded_at IS NOT NULL
                 AND input.recorded_at - latest.recorded_at < (p_min_interval_seconds::DOUBLE PRECISION * INTERVAL '1 second')
                    THEN 'interval'
                ELSE 'candidate'
            END AS decision
        FROM input
        LEFT JOIN existing_parent
          ON existing_parent.user_id = input.user_id
         AND existing_parent.recorded_at = input.recorded_at
        LEFT JOIN LATERAL (
            SELECT recorded_at, followers, following, likes, media_posts, tweets, listed
            FROM tweet.user_stats AS latest
            WHERE latest.user_id = input.user_id
              AND latest.recorded_at <= input.recorded_at
            ORDER BY latest.recorded_at DESC
            LIMIT 1
        ) AS latest ON true
    ),
    inserted AS (
        INSERT INTO tweet.user_stats (
            user_id,
            recorded_at,
            followers,
            following,
            likes,
            media_posts,
            tweets,
            listed
        )
        SELECT
            user_id,
            recorded_at,
            followers,
            following,
            likes,
            media_posts,
            tweets,
            listed
        FROM classified
        WHERE decision = 'candidate'
        ON CONFLICT (user_id, recorded_at) DO NOTHING
        RETURNING user_id, recorded_at
    )
    SELECT
        classified.user_id,
        classified.recorded_at,
        CASE
            WHEN classified.decision = 'missing_parent' THEN 'missing_parent'
            WHEN inserted.user_id IS NOT NULL THEN 'inserted'
            WHEN classified.decision = 'unchanged' THEN 'unchanged'
            WHEN classified.decision = 'interval' THEN 'interval'
            ELSE 'duplicate'
        END AS status
    FROM classified
    LEFT JOIN inserted
      ON inserted.user_id = classified.user_id
     AND inserted.recorded_at = classified.recorded_at;
$$;

CREATE OR REPLACE FUNCTION tweet.write_tweet_places(p_places JSONB)
RETURNS TABLE(id TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.id)
            item.id,
            item.name,
            item.full_name,
            item.country_id,
            item.country_code_id,
            item.kind_id,
            item.boundary
        FROM jsonb_to_recordset(p_places) AS item(
            id TEXT,
            name TEXT,
            full_name TEXT,
            country_id SMALLINT,
            country_code_id SMALLINT,
            kind_id SMALLINT,
            boundary tweet.geo_point[]
        )
        ORDER BY item.id
    ),
    changed AS (
        INSERT INTO tweet.tweet_place (
            id,
            name,
            full_name,
            country_id,
            country_code_id,
            kind_id,
            boundary
        )
        SELECT
            input.id,
            input.name,
            input.full_name,
            input.country_id,
            input.country_code_id,
            input.kind_id,
            input.boundary
        FROM input
        ON CONFLICT (id) DO UPDATE
        SET name = COALESCE(tweet.tweet_place.name, EXCLUDED.name),
            full_name = COALESCE(tweet.tweet_place.full_name, EXCLUDED.full_name),
            country_id = COALESCE(tweet.tweet_place.country_id, EXCLUDED.country_id),
            country_code_id = COALESCE(tweet.tweet_place.country_code_id, EXCLUDED.country_code_id),
            kind_id = COALESCE(tweet.tweet_place.kind_id, EXCLUDED.kind_id),
            boundary = COALESCE(tweet.tweet_place.boundary, EXCLUDED.boundary),
            updated_at = NOW()
        WHERE (tweet.tweet_place.name IS NULL AND EXCLUDED.name IS NOT NULL)
           OR (tweet.tweet_place.full_name IS NULL AND EXCLUDED.full_name IS NOT NULL)
           OR (tweet.tweet_place.country_id IS NULL AND EXCLUDED.country_id IS NOT NULL)
           OR (tweet.tweet_place.country_code_id IS NULL AND EXCLUDED.country_code_id IS NOT NULL)
           OR (tweet.tweet_place.kind_id IS NULL AND EXCLUDED.kind_id IS NOT NULL)
           OR (tweet.tweet_place.boundary IS NULL AND EXCLUDED.boundary IS NOT NULL)
        RETURNING id
    )
    SELECT changed.id
    FROM changed;
$$;

CREATE OR REPLACE FUNCTION tweet.write_tweets(p_tweets JSONB)
RETURNS TABLE(id BIGINT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.id)
            item.id,
            item.published_at,
            item.source_id,
            item.author_id,
            item.place_id,
            item.legacy_text,
            item.note_id,
            item.note_text,
            item.language_id,
            item.conversation_id,
            item.reply_to_tweet_id,
            item.reply_to_user_id,
            item.quote_tweet_id,
            item.quote_permalink,
            item.repost_id
        FROM jsonb_to_recordset(p_tweets) AS item(
            id BIGINT,
            published_at TIMESTAMPTZ,
            source_id SMALLINT,
            author_id BIGINT,
            place_id TEXT,
            legacy_text tweet.annotated_text,
            note_id TEXT,
            note_text tweet.annotated_text,
            language_id SMALLINT,
            conversation_id BIGINT,
            reply_to_tweet_id BIGINT,
            reply_to_user_id BIGINT,
            quote_tweet_id BIGINT,
            quote_permalink tweet.resolved_url,
            repost_id BIGINT
        )
        ORDER BY item.id
    ),
    changed AS (
        INSERT INTO tweet.tweet (
            id,
            published_at,
            source_id,
            author_id,
            place_id,
            legacy_text,
            note_id,
            note_text,
            language_id,
            conversation_id,
            reply_to_tweet_id,
            reply_to_user_id,
            quote_tweet_id,
            quote_permalink,
            repost_id
        )
        SELECT
            input.id,
            input.published_at,
            input.source_id,
            input.author_id,
            input.place_id,
            input.legacy_text,
            input.note_id,
            input.note_text,
            input.language_id,
            input.conversation_id,
            input.reply_to_tweet_id,
            input.reply_to_user_id,
            input.quote_tweet_id,
            input.quote_permalink,
            input.repost_id
        FROM input
        ON CONFLICT (id) DO UPDATE
        SET source_id = COALESCE(tweet.tweet.source_id, EXCLUDED.source_id),
            place_id = COALESCE(tweet.tweet.place_id, EXCLUDED.place_id),
            note_id = COALESCE(tweet.tweet.note_id, EXCLUDED.note_id),
            note_text = COALESCE(tweet.tweet.note_text, EXCLUDED.note_text),
            language_id = COALESCE(tweet.tweet.language_id, EXCLUDED.language_id),
            reply_to_tweet_id = COALESCE(tweet.tweet.reply_to_tweet_id, EXCLUDED.reply_to_tweet_id),
            reply_to_user_id = COALESCE(tweet.tweet.reply_to_user_id, EXCLUDED.reply_to_user_id),
            quote_tweet_id = COALESCE(tweet.tweet.quote_tweet_id, EXCLUDED.quote_tweet_id),
            quote_permalink = COALESCE(tweet.tweet.quote_permalink, EXCLUDED.quote_permalink),
            repost_id = COALESCE(tweet.tweet.repost_id, EXCLUDED.repost_id),
            updated_at = NOW()
        WHERE (tweet.tweet.source_id IS NULL AND EXCLUDED.source_id IS NOT NULL)
           OR (tweet.tweet.place_id IS NULL AND EXCLUDED.place_id IS NOT NULL)
           OR (tweet.tweet.note_id IS NULL AND EXCLUDED.note_id IS NOT NULL)
           OR (tweet.tweet.note_text IS NULL AND EXCLUDED.note_text IS NOT NULL)
           OR (tweet.tweet.language_id IS NULL AND EXCLUDED.language_id IS NOT NULL)
           OR (tweet.tweet.reply_to_tweet_id IS NULL AND EXCLUDED.reply_to_tweet_id IS NOT NULL)
           OR (tweet.tweet.reply_to_user_id IS NULL AND EXCLUDED.reply_to_user_id IS NOT NULL)
           OR (tweet.tweet.quote_tweet_id IS NULL AND EXCLUDED.quote_tweet_id IS NOT NULL)
           OR (tweet.tweet.quote_permalink IS NULL AND EXCLUDED.quote_permalink IS NOT NULL)
           OR (tweet.tweet.repost_id IS NULL AND EXCLUDED.repost_id IS NOT NULL)
        RETURNING id
    )
    SELECT changed.id
    FROM changed;
$$;

CREATE OR REPLACE FUNCTION tweet.write_tweet_edits(p_edits JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.tweet_id)
            item.tweet_id,
            COALESCE(item.version_ids, ARRAY[]::BIGINT[]) AS version_ids,
            item.editable_until,
            item.remaining_edits
        FROM jsonb_to_recordset(p_edits) AS item(
            tweet_id BIGINT,
            version_ids BIGINT[],
            editable_until TIMESTAMPTZ,
            remaining_edits INTEGER
        )
        ORDER BY item.tweet_id
    ),
    existing_parent AS (
        SELECT input.tweet_id
        FROM input
        JOIN tweet.tweet AS parent
          ON parent.id = input.tweet_id
    ),
    changed AS (
        INSERT INTO tweet.tweet_edit (
            tweet_id,
            version_ids,
            editable_until,
            remaining_edits
        )
        SELECT
            input.tweet_id,
            input.version_ids,
            input.editable_until,
            input.remaining_edits
        FROM input
        JOIN existing_parent
          ON existing_parent.tweet_id = input.tweet_id
        ON CONFLICT (tweet_id) DO UPDATE
        SET version_ids = CASE
                WHEN COALESCE(cardinality(tweet.tweet_edit.version_ids), 0) = 0
                 AND COALESCE(cardinality(EXCLUDED.version_ids), 0) > 0
                THEN EXCLUDED.version_ids
                ELSE tweet.tweet_edit.version_ids
            END,
            editable_until = COALESCE(tweet.tweet_edit.editable_until, EXCLUDED.editable_until),
            remaining_edits = COALESCE(tweet.tweet_edit.remaining_edits, EXCLUDED.remaining_edits),
            updated_at = NOW()
        WHERE (
                COALESCE(cardinality(tweet.tweet_edit.version_ids), 0) = 0
            AND COALESCE(cardinality(EXCLUDED.version_ids), 0) > 0
        )
           OR (tweet.tweet_edit.editable_until IS NULL AND EXCLUDED.editable_until IS NOT NULL)
           OR (tweet.tweet_edit.remaining_edits IS NULL AND EXCLUDED.remaining_edits IS NOT NULL)
        RETURNING tweet_id
    )
    SELECT
        input.tweet_id,
        CASE
            WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
            WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
            ELSE 'unchanged'
        END AS status
    FROM input
    LEFT JOIN existing_parent
      ON existing_parent.tweet_id = input.tweet_id
    LEFT JOIN changed
      ON changed.tweet_id = input.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.write_tweet_policies(p_policies JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.tweet_id)
            item.tweet_id,
            item.reply_policy_id,
            item.followers_only,
            item.is_possibly_sensitive,
            COALESCE(item.available_action_ids, ARRAY[]::SMALLINT[]) AS available_action_ids,
            item.is_media_visibility_restricted,
            item.paid_promotion
        FROM jsonb_to_recordset(p_policies) AS item(
            tweet_id BIGINT,
            reply_policy_id SMALLINT,
            followers_only BOOLEAN,
            is_possibly_sensitive BOOLEAN,
            available_action_ids SMALLINT[],
            is_media_visibility_restricted BOOLEAN,
            paid_promotion BOOLEAN
        )
        ORDER BY item.tweet_id
    ),
    existing_parent AS (
        SELECT input.tweet_id
        FROM input
        JOIN tweet.tweet AS parent
          ON parent.id = input.tweet_id
    ),
    changed AS (
        INSERT INTO tweet.tweet_policy (
            tweet_id,
            reply_policy_id,
            followers_only,
            is_possibly_sensitive,
            available_action_ids,
            is_media_visibility_restricted,
            paid_promotion
        )
        SELECT
            input.tweet_id,
            input.reply_policy_id,
            input.followers_only,
            input.is_possibly_sensitive,
            input.available_action_ids,
            input.is_media_visibility_restricted,
            input.paid_promotion
        FROM input
        JOIN existing_parent
          ON existing_parent.tweet_id = input.tweet_id
        ON CONFLICT (tweet_id) DO UPDATE
        SET reply_policy_id = COALESCE(tweet.tweet_policy.reply_policy_id, EXCLUDED.reply_policy_id),
            followers_only = COALESCE(tweet.tweet_policy.followers_only, EXCLUDED.followers_only),
            is_possibly_sensitive = COALESCE(tweet.tweet_policy.is_possibly_sensitive, EXCLUDED.is_possibly_sensitive),
            available_action_ids = CASE
                WHEN COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
                 AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
                THEN EXCLUDED.available_action_ids
                ELSE tweet.tweet_policy.available_action_ids
            END,
            is_media_visibility_restricted = COALESCE(tweet.tweet_policy.is_media_visibility_restricted, EXCLUDED.is_media_visibility_restricted),
            paid_promotion = COALESCE(tweet.tweet_policy.paid_promotion, EXCLUDED.paid_promotion),
            updated_at = NOW()
        WHERE (tweet.tweet_policy.reply_policy_id IS NULL AND EXCLUDED.reply_policy_id IS NOT NULL)
           OR (tweet.tweet_policy.followers_only IS NULL AND EXCLUDED.followers_only IS NOT NULL)
           OR (tweet.tweet_policy.is_possibly_sensitive IS NULL AND EXCLUDED.is_possibly_sensitive IS NOT NULL)
           OR (
                COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
            AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
           )
           OR (
                tweet.tweet_policy.is_media_visibility_restricted IS NULL
            AND EXCLUDED.is_media_visibility_restricted IS NOT NULL
           )
           OR (tweet.tweet_policy.paid_promotion IS NULL AND EXCLUDED.paid_promotion IS NOT NULL)
        RETURNING tweet_id
    )
    SELECT
        input.tweet_id,
        CASE
            WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
            WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
            ELSE 'unchanged'
        END AS status
    FROM input
    LEFT JOIN existing_parent
      ON existing_parent.tweet_id = input.tweet_id
    LEFT JOIN changed
      ON changed.tweet_id = input.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.append_tweet_stats_if_changed(p_stats JSONB, p_min_interval_seconds BIGINT)
RETURNS TABLE(tweet_id BIGINT, recorded_at TIMESTAMPTZ, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.tweet_id, item.recorded_at)
            item.tweet_id,
            item.recorded_at,
            item.views,
            item.replies,
            item.reposts,
            item.quotes,
            item.likes,
            item.bookmarks
        FROM jsonb_to_recordset(p_stats) AS item(
            tweet_id BIGINT,
            recorded_at TIMESTAMPTZ,
            views BIGINT,
            replies BIGINT,
            reposts BIGINT,
            quotes BIGINT,
            likes BIGINT,
            bookmarks BIGINT
        )
        ORDER BY item.tweet_id, item.recorded_at
    ),
    existing_parent AS (
        SELECT input.tweet_id, input.recorded_at
        FROM input
        JOIN tweet.tweet AS parent
          ON parent.id = input.tweet_id
    ),
    classified AS (
        SELECT
            input.*,
            CASE
                WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
                WHEN latest.recorded_at IS NOT NULL
                 AND latest.views IS NOT DISTINCT FROM input.views
                 AND latest.replies IS NOT DISTINCT FROM input.replies
                 AND latest.reposts IS NOT DISTINCT FROM input.reposts
                 AND latest.quotes IS NOT DISTINCT FROM input.quotes
                 AND latest.likes IS NOT DISTINCT FROM input.likes
                 AND latest.bookmarks IS NOT DISTINCT FROM input.bookmarks
                    THEN 'unchanged'
                WHEN latest.recorded_at IS NOT NULL
                 AND input.recorded_at - latest.recorded_at < (p_min_interval_seconds::DOUBLE PRECISION * INTERVAL '1 second')
                    THEN 'interval'
                ELSE 'candidate'
            END AS decision
        FROM input
        LEFT JOIN existing_parent
          ON existing_parent.tweet_id = input.tweet_id
         AND existing_parent.recorded_at = input.recorded_at
        LEFT JOIN LATERAL (
            SELECT recorded_at, views, replies, reposts, quotes, likes, bookmarks
            FROM tweet.tweet_stats AS latest
            WHERE latest.tweet_id = input.tweet_id
              AND latest.recorded_at <= input.recorded_at
            ORDER BY latest.recorded_at DESC
            LIMIT 1
        ) AS latest ON true
    ),
    inserted AS (
        INSERT INTO tweet.tweet_stats (
            tweet_id,
            recorded_at,
            views,
            replies,
            reposts,
            quotes,
            likes,
            bookmarks
        )
        SELECT
            tweet_id,
            recorded_at,
            views,
            replies,
            reposts,
            quotes,
            likes,
            bookmarks
        FROM classified
        WHERE decision = 'candidate'
        ON CONFLICT (tweet_id, recorded_at) DO NOTHING
        RETURNING tweet_id, recorded_at
    )
    SELECT
        classified.tweet_id,
        classified.recorded_at,
        CASE
            WHEN classified.decision = 'missing_parent' THEN 'missing_parent'
            WHEN inserted.tweet_id IS NOT NULL THEN 'inserted'
            WHEN classified.decision = 'unchanged' THEN 'unchanged'
            WHEN classified.decision = 'interval' THEN 'interval'
            ELSE 'duplicate'
        END AS status
    FROM classified
    LEFT JOIN inserted
      ON inserted.tweet_id = classified.tweet_id
     AND inserted.recorded_at = classified.recorded_at;
$$;

CREATE OR REPLACE FUNCTION tweet.write_tweet_community_notes(p_notes JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.tweet_id)
            item.tweet_id,
            item.note_id,
            item.title,
            item.short_title,
            item.subtitle,
            item.footer,
            item.destination_url
        FROM jsonb_to_recordset(p_notes) AS item(
            tweet_id BIGINT,
            note_id BIGINT,
            title TEXT,
            short_title TEXT,
            subtitle tweet.annotated_text,
            footer tweet.annotated_text,
            destination_url TEXT
        )
        ORDER BY item.tweet_id
    ),
    existing_parent AS (
        SELECT input.tweet_id
        FROM input
        JOIN tweet.tweet AS parent
          ON parent.id = input.tweet_id
    ),
    changed AS (
        INSERT INTO tweet.tweet_community_note (
            tweet_id,
            note_id,
            title,
            short_title,
            subtitle,
            footer,
            destination_url
        )
        SELECT
            input.tweet_id,
            input.note_id,
            input.title,
            input.short_title,
            input.subtitle,
            input.footer,
            input.destination_url
        FROM input
        JOIN existing_parent
          ON existing_parent.tweet_id = input.tweet_id
        ON CONFLICT (tweet_id) DO UPDATE
        SET note_id = COALESCE(tweet.tweet_community_note.note_id, EXCLUDED.note_id),
            title = COALESCE(tweet.tweet_community_note.title, EXCLUDED.title),
            short_title = COALESCE(tweet.tweet_community_note.short_title, EXCLUDED.short_title),
            subtitle = COALESCE(tweet.tweet_community_note.subtitle, EXCLUDED.subtitle),
            footer = COALESCE(tweet.tweet_community_note.footer, EXCLUDED.footer),
            destination_url = COALESCE(tweet.tweet_community_note.destination_url, EXCLUDED.destination_url),
            updated_at = NOW()
        WHERE (tweet.tweet_community_note.note_id IS NULL AND EXCLUDED.note_id IS NOT NULL)
           OR (tweet.tweet_community_note.title IS NULL AND EXCLUDED.title IS NOT NULL)
           OR (tweet.tweet_community_note.short_title IS NULL AND EXCLUDED.short_title IS NOT NULL)
           OR (tweet.tweet_community_note.subtitle IS NULL AND EXCLUDED.subtitle IS NOT NULL)
           OR (tweet.tweet_community_note.footer IS NULL AND EXCLUDED.footer IS NOT NULL)
           OR (
                tweet.tweet_community_note.destination_url IS NULL
            AND EXCLUDED.destination_url IS NOT NULL
           )
        RETURNING tweet_id
    )
    SELECT
        input.tweet_id,
        CASE
            WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
            WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
            ELSE 'unchanged'
        END AS status
    FROM input
    LEFT JOIN existing_parent
      ON existing_parent.tweet_id = input.tweet_id
    LEFT JOIN changed
      ON changed.tweet_id = input.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.write_media(p_media JSONB)
RETURNS TABLE(id BIGINT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.id)
            item.id,
            item.media_type,
            item.alt_text,
            item.grok_post_id,
            item.geometry,
            item.size_variants,
            COALESCE(item.tagged_users, ARRAY[]::tweet.media_tag[]) AS tagged_users,
            COALESCE(item.sensitivity_warning_ids, ARRAY[]::SMALLINT[]) AS sensitivity_warning_ids,
            item.origin_tweet_id,
            item.origin_user_id,
            item.details
        FROM jsonb_to_recordset(p_media) AS item(
            id BIGINT,
            media_type tweet.media_type_enum,
            alt_text TEXT,
            grok_post_id UUID,
            geometry tweet.media_geometry,
            size_variants tweet.media_size_variants,
            tagged_users tweet.media_tag[],
            sensitivity_warning_ids SMALLINT[],
            origin_tweet_id BIGINT,
            origin_user_id BIGINT,
            details tweet.media_details
        )
        ORDER BY item.id
    ),
    changed AS (
        INSERT INTO tweet.media (
            id,
            type,
            alt_text,
            grok_post_id,
            geometry,
            size_variants,
            tagged_users,
            sensitivity_warning_ids,
            origin_tweet_id,
            origin_user_id,
            details
        )
        SELECT
            input.id,
            input.media_type,
            input.alt_text,
            input.grok_post_id,
            input.geometry,
            input.size_variants,
            input.tagged_users,
            input.sensitivity_warning_ids,
            input.origin_tweet_id,
            input.origin_user_id,
            input.details
        FROM input
        ON CONFLICT (id) DO UPDATE
        SET alt_text = COALESCE(tweet.media.alt_text, EXCLUDED.alt_text),
            grok_post_id = COALESCE(tweet.media.grok_post_id, EXCLUDED.grok_post_id),
            geometry = COALESCE(tweet.media.geometry, EXCLUDED.geometry),
            size_variants = COALESCE(tweet.media.size_variants, EXCLUDED.size_variants),
            tagged_users = CASE
                WHEN COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
                 AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
                THEN EXCLUDED.tagged_users
                ELSE tweet.media.tagged_users
            END,
            sensitivity_warning_ids = CASE
                WHEN COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
                 AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
                THEN EXCLUDED.sensitivity_warning_ids
                ELSE tweet.media.sensitivity_warning_ids
            END,
            origin_tweet_id = COALESCE(tweet.media.origin_tweet_id, EXCLUDED.origin_tweet_id),
            origin_user_id = COALESCE(tweet.media.origin_user_id, EXCLUDED.origin_user_id),
            details = COALESCE(tweet.media.details, EXCLUDED.details),
            updated_at = NOW()
        WHERE (tweet.media.alt_text IS NULL AND EXCLUDED.alt_text IS NOT NULL)
           OR (tweet.media.grok_post_id IS NULL AND EXCLUDED.grok_post_id IS NOT NULL)
           OR (tweet.media.geometry IS NULL AND EXCLUDED.geometry IS NOT NULL)
           OR (tweet.media.size_variants IS NULL AND EXCLUDED.size_variants IS NOT NULL)
           OR (
                COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
            AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
           )
           OR (
                COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
            AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
           )
           OR (tweet.media.origin_tweet_id IS NULL AND EXCLUDED.origin_tweet_id IS NOT NULL)
           OR (tweet.media.origin_user_id IS NULL AND EXCLUDED.origin_user_id IS NOT NULL)
           OR (tweet.media.details IS NULL AND EXCLUDED.details IS NOT NULL)
        RETURNING id
    )
    SELECT changed.id
    FROM changed;
$$;

CREATE OR REPLACE FUNCTION tweet.append_media_resources_if_changed(p_resources JSONB)
RETURNS TABLE(media_id BIGINT, recorded_at TIMESTAMPTZ, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.media_id, item.recorded_at)
            item.media_id,
            item.recorded_at,
            item.media_url,
            item.availability_id,
            item.video
        FROM jsonb_to_recordset(p_resources) AS item(
            media_id BIGINT,
            recorded_at TIMESTAMPTZ,
            media_url TEXT,
            availability_id SMALLINT,
            video tweet.media_video
        )
        ORDER BY item.media_id, item.recorded_at
    ),
    existing_parent AS (
        SELECT input.media_id, input.recorded_at
        FROM input
        JOIN tweet.media AS parent
          ON parent.id = input.media_id
    ),
    classified AS (
        SELECT
            input.*,
            (existing_parent.media_id IS NOT NULL) AS has_parent,
            (
                existing_parent.media_id IS NOT NULL
            AND latest.media_id IS NOT NULL
            AND latest.media_url IS NOT DISTINCT FROM input.media_url
            AND latest.availability_id IS NOT DISTINCT FROM input.availability_id
            AND to_jsonb(latest.video) IS NOT DISTINCT FROM to_jsonb(input.video)
            ) AS unchanged
        FROM input
        LEFT JOIN existing_parent
          ON existing_parent.media_id = input.media_id
         AND existing_parent.recorded_at = input.recorded_at
        LEFT JOIN LATERAL (
            SELECT *
            FROM tweet.media_resource AS latest
            WHERE latest.media_id = input.media_id
              AND latest.recorded_at <= input.recorded_at
            ORDER BY latest.recorded_at DESC
            LIMIT 1
        ) AS latest ON true
    ),
    inserted AS (
        INSERT INTO tweet.media_resource (
            media_id,
            recorded_at,
            media_url,
            availability_id,
            video
        )
        SELECT
            media_id,
            recorded_at,
            media_url,
            availability_id,
            video
        FROM classified
        WHERE has_parent
          AND NOT unchanged
        ON CONFLICT (media_id, recorded_at) DO NOTHING
        RETURNING media_id, recorded_at
    )
    SELECT
        classified.media_id,
        classified.recorded_at,
        CASE
            WHEN NOT classified.has_parent THEN 'missing_parent'
            WHEN inserted.media_id IS NOT NULL THEN 'inserted'
            WHEN classified.unchanged THEN 'unchanged'
            ELSE 'duplicate'
        END AS status
    FROM classified
    LEFT JOIN inserted
      ON inserted.media_id = classified.media_id
     AND inserted.recorded_at = classified.recorded_at;
$$;

CREATE OR REPLACE FUNCTION tweet.sync_tweet_relations(
    p_tweet_ids BIGINT[],
    p_media_refs JSONB,
    p_mention_refs JSONB,
    p_hashtag_refs JSONB,
    p_symbol_refs JSONB
)
RETURNS TABLE(
    tweet_id BIGINT,
    media_status TEXT,
    mention_status TEXT,
    hashtag_status TEXT,
    symbol_status TEXT
)
LANGUAGE sql
VOLATILE
AS $$
    WITH target_tweets AS (
        SELECT DISTINCT UNNEST(p_tweet_ids) AS tweet_id
    ),
    media_sync AS (
        SELECT *
        FROM tweet.sync_tweet_media_refs(p_tweet_ids, p_media_refs)
    ),
    mention_sync AS (
        SELECT *
        FROM tweet.sync_tweet_mention_refs(p_tweet_ids, p_mention_refs)
    ),
    hashtag_sync AS (
        SELECT *
        FROM tweet.sync_tweet_hashtag_refs(p_tweet_ids, p_hashtag_refs)
    ),
    symbol_sync AS (
        SELECT *
        FROM tweet.sync_tweet_symbol_refs(p_tweet_ids, p_symbol_refs)
    )
    SELECT
        target_tweets.tweet_id,
        media_sync.status AS media_status,
        mention_sync.status AS mention_status,
        hashtag_sync.status AS hashtag_status,
        symbol_sync.status AS symbol_status
    FROM target_tweets
    LEFT JOIN media_sync
      ON media_sync.tweet_id = target_tweets.tweet_id
    LEFT JOIN mention_sync
      ON mention_sync.tweet_id = target_tweets.tweet_id
    LEFT JOIN hashtag_sync
      ON hashtag_sync.tweet_id = target_tweets.tweet_id
    LEFT JOIN symbol_sync
      ON symbol_sync.tweet_id = target_tweets.tweet_id;
$$;
