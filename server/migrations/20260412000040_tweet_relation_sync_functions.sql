CREATE OR REPLACE FUNCTION tweet.sync_tweet_media_refs(p_tweet_ids BIGINT[], p_refs JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH target_tweets AS (
        SELECT DISTINCT UNNEST(p_tweet_ids) AS tweet_id
    ),
    existing_tweets AS (
        SELECT target_tweets.tweet_id
        FROM target_tweets
        JOIN tweet.tweet AS t
          ON t.id = target_tweets.tweet_id
    ),
    raw_input AS (
        SELECT DISTINCT ON (item.tweet_id, item.media_id)
            item.tweet_id,
            item.media_id,
            item.display_order
        FROM jsonb_to_recordset(p_refs) AS item(
            tweet_id BIGINT,
            media_id BIGINT,
            display_order SMALLINT
        )
        JOIN existing_tweets
          ON existing_tweets.tweet_id = item.tweet_id
        ORDER BY item.tweet_id, item.media_id, item.display_order
    ),
    input_refs AS (
        SELECT raw_input.tweet_id, raw_input.media_id, raw_input.display_order
        FROM raw_input
        JOIN tweet.media AS m
          ON m.id = raw_input.media_id
    ),
    requested_counts AS (
        SELECT tweet_id, COUNT(*)::INTEGER AS requested_count
        FROM raw_input
        GROUP BY tweet_id
    ),
    effective_counts AS (
        SELECT tweet_id, COUNT(*)::INTEGER AS effective_count
        FROM input_refs
        GROUP BY tweet_id
    ),
    desired_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'media_id', input_refs.media_id,
                        'display_order', input_refs.display_order
                    )
                    ORDER BY input_refs.display_order, input_refs.media_id
                ) FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN input_refs
          ON input_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    current_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'media_id', current_refs.media_id,
                        'display_order', current_refs.display_order
                    )
                    ORDER BY current_refs.display_order, current_refs.media_id
                ) FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN tweet.tweet_media_ref AS current_refs
          ON current_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    changed_tweets AS (
        SELECT
            desired_state.tweet_id,
            COALESCE(requested_counts.requested_count, 0)
                > COALESCE(effective_counts.effective_count, 0) AS filtered
        FROM desired_state
        JOIN current_state
          ON current_state.tweet_id = desired_state.tweet_id
        LEFT JOIN requested_counts
          ON requested_counts.tweet_id = desired_state.tweet_id
        LEFT JOIN effective_counts
          ON effective_counts.tweet_id = desired_state.tweet_id
        WHERE desired_state.refs IS DISTINCT FROM current_state.refs
    ),
    deleted AS (
        DELETE FROM tweet.tweet_media_ref
        WHERE tweet_id IN (SELECT changed_tweets.tweet_id FROM changed_tweets)
    ),
    inserted AS (
        INSERT INTO tweet.tweet_media_ref (tweet_id, media_id, display_order)
        SELECT input_refs.tweet_id, input_refs.media_id, input_refs.display_order
        FROM input_refs
        JOIN changed_tweets
          ON changed_tweets.tweet_id = input_refs.tweet_id
    )
    SELECT
        target_tweets.tweet_id,
        CASE
            WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
            WHEN changed_tweets.tweet_id IS NOT NULL AND changed_tweets.filtered THEN 'replaced_filtered'
            WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
            WHEN COALESCE(requested_counts.requested_count, 0)
               > COALESCE(effective_counts.effective_count, 0) THEN 'unchanged_filtered'
            ELSE 'unchanged'
        END AS status
    FROM target_tweets
    LEFT JOIN existing_tweets
      ON existing_tweets.tweet_id = target_tweets.tweet_id
    LEFT JOIN changed_tweets
      ON changed_tweets.tweet_id = target_tweets.tweet_id
    LEFT JOIN requested_counts
      ON requested_counts.tweet_id = target_tweets.tweet_id
    LEFT JOIN effective_counts
      ON effective_counts.tweet_id = target_tweets.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.sync_tweet_mention_refs(p_tweet_ids BIGINT[], p_refs JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH target_tweets AS (
        SELECT DISTINCT UNNEST(p_tweet_ids) AS tweet_id
    ),
    existing_tweets AS (
        SELECT target_tweets.tweet_id
        FROM target_tweets
        JOIN tweet.tweet AS t
          ON t.id = target_tweets.tweet_id
    ),
    input_refs AS (
        SELECT DISTINCT ON (item.tweet_id, item.user_id)
            item.tweet_id,
            item.user_id
        FROM jsonb_to_recordset(p_refs) AS item(
            tweet_id BIGINT,
            user_id BIGINT
        )
        JOIN existing_tweets
          ON existing_tweets.tweet_id = item.tweet_id
        ORDER BY item.tweet_id, item.user_id
    ),
    desired_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(input_refs.user_id ORDER BY input_refs.user_id)
                    FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN input_refs
          ON input_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    current_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(current_refs.user_id ORDER BY current_refs.user_id)
                    FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN tweet.tweet_mention_ref AS current_refs
          ON current_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    changed_tweets AS (
        SELECT desired_state.tweet_id
        FROM desired_state
        JOIN current_state
          ON current_state.tweet_id = desired_state.tweet_id
        WHERE desired_state.refs IS DISTINCT FROM current_state.refs
    ),
    deleted AS (
        DELETE FROM tweet.tweet_mention_ref
        WHERE tweet_id IN (SELECT changed_tweets.tweet_id FROM changed_tweets)
    ),
    inserted AS (
        INSERT INTO tweet.tweet_mention_ref (tweet_id, user_id)
        SELECT input_refs.tweet_id, input_refs.user_id
        FROM input_refs
        JOIN changed_tweets
          ON changed_tweets.tweet_id = input_refs.tweet_id
    )
    SELECT
        target_tweets.tweet_id,
        CASE
            WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
            WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
            ELSE 'unchanged'
        END AS status
    FROM target_tweets
    LEFT JOIN existing_tweets
      ON existing_tweets.tweet_id = target_tweets.tweet_id
    LEFT JOIN changed_tweets
      ON changed_tweets.tweet_id = target_tweets.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.sync_tweet_hashtag_refs(p_tweet_ids BIGINT[], p_refs JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH target_tweets AS (
        SELECT DISTINCT UNNEST(p_tweet_ids) AS tweet_id
    ),
    existing_tweets AS (
        SELECT target_tweets.tweet_id
        FROM target_tweets
        JOIN tweet.tweet AS t
          ON t.id = target_tweets.tweet_id
    ),
    input_refs AS (
        SELECT DISTINCT ON (item.tweet_id, item.hashtag_id)
            item.tweet_id,
            item.hashtag_id
        FROM jsonb_to_recordset(p_refs) AS item(
            tweet_id BIGINT,
            hashtag_id INTEGER
        )
        JOIN existing_tweets
          ON existing_tweets.tweet_id = item.tweet_id
        ORDER BY item.tweet_id, item.hashtag_id
    ),
    desired_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(input_refs.hashtag_id ORDER BY input_refs.hashtag_id)
                    FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN input_refs
          ON input_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    current_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(current_refs.hashtag_id ORDER BY current_refs.hashtag_id)
                    FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN tweet.tweet_hashtag_ref AS current_refs
          ON current_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    changed_tweets AS (
        SELECT desired_state.tweet_id
        FROM desired_state
        JOIN current_state
          ON current_state.tweet_id = desired_state.tweet_id
        WHERE desired_state.refs IS DISTINCT FROM current_state.refs
    ),
    deleted AS (
        DELETE FROM tweet.tweet_hashtag_ref
        WHERE tweet_id IN (SELECT changed_tweets.tweet_id FROM changed_tweets)
    ),
    inserted AS (
        INSERT INTO tweet.tweet_hashtag_ref (tweet_id, hashtag_id)
        SELECT input_refs.tweet_id, input_refs.hashtag_id
        FROM input_refs
        JOIN changed_tweets
          ON changed_tweets.tweet_id = input_refs.tweet_id
    )
    SELECT
        target_tweets.tweet_id,
        CASE
            WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
            WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
            ELSE 'unchanged'
        END AS status
    FROM target_tweets
    LEFT JOIN existing_tweets
      ON existing_tweets.tweet_id = target_tweets.tweet_id
    LEFT JOIN changed_tweets
      ON changed_tweets.tweet_id = target_tweets.tweet_id;
$$;

CREATE OR REPLACE FUNCTION tweet.sync_tweet_symbol_refs(p_tweet_ids BIGINT[], p_refs JSONB)
RETURNS TABLE(tweet_id BIGINT, status TEXT)
LANGUAGE sql
VOLATILE
AS $$
    WITH target_tweets AS (
        SELECT DISTINCT UNNEST(p_tweet_ids) AS tweet_id
    ),
    existing_tweets AS (
        SELECT target_tweets.tweet_id
        FROM target_tweets
        JOIN tweet.tweet AS t
          ON t.id = target_tweets.tweet_id
    ),
    input_refs AS (
        SELECT DISTINCT ON (item.tweet_id, item.symbol_id)
            item.tweet_id,
            item.symbol_id
        FROM jsonb_to_recordset(p_refs) AS item(
            tweet_id BIGINT,
            symbol_id INTEGER
        )
        JOIN existing_tweets
          ON existing_tweets.tweet_id = item.tweet_id
        ORDER BY item.tweet_id, item.symbol_id
    ),
    desired_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(input_refs.symbol_id ORDER BY input_refs.symbol_id)
                    FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN input_refs
          ON input_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    current_state AS (
        SELECT
            existing_tweets.tweet_id,
            COALESCE(
                jsonb_agg(current_refs.symbol_id ORDER BY current_refs.symbol_id)
                    FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                '[]'::jsonb
            ) AS refs
        FROM existing_tweets
        LEFT JOIN tweet.tweet_symbol_ref AS current_refs
          ON current_refs.tweet_id = existing_tweets.tweet_id
        GROUP BY existing_tweets.tweet_id
    ),
    changed_tweets AS (
        SELECT desired_state.tweet_id
        FROM desired_state
        JOIN current_state
          ON current_state.tweet_id = desired_state.tweet_id
        WHERE desired_state.refs IS DISTINCT FROM current_state.refs
    ),
    deleted AS (
        DELETE FROM tweet.tweet_symbol_ref
        WHERE tweet_id IN (SELECT changed_tweets.tweet_id FROM changed_tweets)
    ),
    inserted AS (
        INSERT INTO tweet.tweet_symbol_ref (tweet_id, symbol_id)
        SELECT input_refs.tweet_id, input_refs.symbol_id
        FROM input_refs
        JOIN changed_tweets
          ON changed_tweets.tweet_id = input_refs.tweet_id
    )
    SELECT
        target_tweets.tweet_id,
        CASE
            WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
            WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
            ELSE 'unchanged'
        END AS status
    FROM target_tweets
    LEFT JOIN existing_tweets
      ON existing_tweets.tweet_id = target_tweets.tweet_id
    LEFT JOIN changed_tweets
      ON changed_tweets.tweet_id = target_tweets.tweet_id;
$$;
