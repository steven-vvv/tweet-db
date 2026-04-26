CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;

ALTER TABLE search.index_queue
ALTER COLUMN id SET DEFAULT gen_random_uuid();

CREATE OR REPLACE FUNCTION search.enqueue_targets(p_targets JSONB, p_force_refresh BOOLEAN DEFAULT TRUE)
RETURNS TABLE(target_kind search.index_target_kind, target_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH input AS (
        SELECT DISTINCT ON (item.target_kind, item.target_id)
            item.target_kind::search.index_target_kind AS target_kind,
            item.target_id
        FROM jsonb_to_recordset(COALESCE(p_targets, '[]'::jsonb)) AS item(
            target_kind TEXT,
            target_id BIGINT
        )
        WHERE item.target_kind IS NOT NULL
          AND item.target_id IS NOT NULL
        ORDER BY item.target_kind, item.target_id
    ),
    upserted AS (
        INSERT INTO search.index_queue AS queue (
            target_kind,
            target_id,
            status
        )
        SELECT
            input.target_kind,
            input.target_id,
            'pending'::search.index_task_status
        FROM input
        ON CONFLICT (target_kind, target_id) DO UPDATE
        SET status = CASE
                WHEN p_force_refresh THEN 'pending'::search.index_task_status
                ELSE queue.status
            END,
            attempt_count = CASE
                WHEN p_force_refresh THEN 0
                ELSE queue.attempt_count
            END,
            last_error = CASE
                WHEN p_force_refresh THEN NULL
                ELSE queue.last_error
            END,
            claimed_by = CASE
                WHEN p_force_refresh THEN NULL
                ELSE queue.claimed_by
            END,
            claimed_at = CASE
                WHEN p_force_refresh THEN NULL
                ELSE queue.claimed_at
            END,
            completed_at = CASE
                WHEN p_force_refresh THEN NULL
                ELSE queue.completed_at
            END,
            updated_at = CASE
                WHEN p_force_refresh THEN NOW()
                ELSE queue.updated_at
            END
        RETURNING
            queue.target_kind,
            queue.target_id,
            (xmax = 0) AS inserted
    )
    SELECT
        upserted.target_kind,
        upserted.target_id,
        CASE WHEN upserted.inserted THEN 'enqueued' ELSE 'refreshed' END AS status
    FROM upserted;
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
    ),
    queued AS (
        SELECT queue.target_id AS id
        FROM search.enqueue_targets(
            (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'target_kind', 'user',
                        'target_id', changed.id
                    )
                )
                FROM changed
            ),
            TRUE
        ) AS queue
    )
    SELECT changed.id
    FROM changed
    LEFT JOIN queued
      ON queued.id = changed.id;
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
    ),
    queued AS (
        SELECT queue.target_id AS user_id
        FROM search.enqueue_targets(
            (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'target_kind', 'user',
                        'target_id', inserted.user_id
                    )
                )
                FROM inserted
            ),
            TRUE
        ) AS queue
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
     AND inserted.recorded_at = classified.recorded_at
    LEFT JOIN queued
      ON queued.user_id = inserted.user_id;
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
    ),
    queued AS (
        SELECT queue.target_id AS id
        FROM search.enqueue_targets(
            (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'target_kind', 'tweet',
                        'target_id', changed.id
                    )
                )
                FROM changed
            ),
            TRUE
        ) AS queue
    )
    SELECT changed.id
    FROM changed
    LEFT JOIN queued
      ON queued.id = changed.id;
$$;
