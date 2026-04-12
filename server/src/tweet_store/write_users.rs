use super::*;

impl<'a> TweetStore<'a> {
    pub async fn insert_users(&self, users: &[TwitterUser]) -> AppResult<u64> {
        if users.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.twitter_user (id, registered_at)
            SELECT item.id, item.registered_at
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id BIGINT,
                registered_at TIMESTAMPTZ
            )
            ON CONFLICT (id) DO UPDATE
            SET registered_at = EXCLUDED.registered_at,
                updated_at = NOW()
            WHERE tweet.twitter_user.registered_at IS NULL
              AND EXCLUDED.registered_at IS NOT NULL
            "#,
        )
        .bind(serde_json::to_value(users)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn insert_users_changed(&self, users: &[TwitterUser]) -> AppResult<HashSet<i64>> {
        if users.is_empty() {
            return Ok(HashSet::new());
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.id) item.id, item.registered_at
                FROM jsonb_to_recordset($1::jsonb) AS item(
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
            SELECT id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(users)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }

    pub async fn append_user_snapshots(&self, snapshots: &[UserSnapshot]) -> AppResult<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }

        self.preload_user_snapshot_dicts(snapshots).await?;
        let mut payloads = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            payloads.push(self.user_snapshot_payload(snapshot).await?);
        }

        let result = sqlx::query(
            r#"
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
                COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]),
                item.identity,
                item.features,
                item.professional,
                COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[])
            FROM jsonb_to_recordset($1::jsonb) AS item(
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
            ON CONFLICT (user_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_user_stats(&self, stats: &[UserStats]) -> AppResult<u64> {
        if stats.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
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
                item.user_id,
                item.recorded_at,
                item.followers,
                item.following,
                item.likes,
                item.media_posts,
                item.tweets,
                item.listed
            FROM jsonb_to_recordset($1::jsonb) AS item(
                user_id BIGINT,
                recorded_at TIMESTAMPTZ,
                followers BIGINT,
                following BIGINT,
                likes BIGINT,
                media_posts BIGINT,
                tweets BIGINT,
                listed BIGINT
            )
            ON CONFLICT (user_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(stats)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_user_stats_if_changed(
        &self,
        stats: &UserStats,
        min_interval_seconds: i64,
    ) -> AppResult<ConditionalWrite> {
        if let Some(previous) = sqlx::query_as::<_, UserStatsRow>(
            r#"
            SELECT recorded_at, followers, following, likes, media_posts, tweets, listed
            FROM tweet.user_stats
            WHERE user_id = $1
              AND recorded_at <= $2
            ORDER BY recorded_at DESC
            LIMIT 1
            "#,
        )
        .bind(stats.user_id)
        .bind(stats.recorded_at)
        .fetch_optional(self.pool)
        .await?
        {
            if previous.same_user_stats(stats) {
                return Ok(ConditionalWrite::SkippedUnchanged);
            }

            if stats.recorded_at - previous.recorded_at
                < time::Duration::seconds(min_interval_seconds)
            {
                return Ok(ConditionalWrite::SkippedInterval);
            }
        }

        match self.append_user_stats(std::slice::from_ref(stats)).await? {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }

    pub async fn append_user_stats_if_changed_many(
        &self,
        stats: &[UserStats],
        min_interval_seconds: i64,
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if stats.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
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
                FROM jsonb_to_recordset($1::jsonb) AS item(
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
                         AND input.recorded_at - latest.recorded_at < ($2::DOUBLE PRECISION * INTERVAL '1 second')
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
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(stats)?)
        .bind(min_interval_seconds)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("user_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
    }

    pub async fn append_user_snapshots_if_changed_many(
        &self,
        snapshots: &[UserSnapshot],
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if snapshots.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_user_snapshot_dicts(snapshots).await?;
        let mut payloads = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            payloads.push(self.user_snapshot_payload(snapshot).await?);
        }

        let rows = sqlx::query(
            r#"
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
                FROM jsonb_to_recordset($1::jsonb) AS item(
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
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("user_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
    }

    pub async fn append_media_resource_if_changed(
        &self,
        resource: &MediaResource,
    ) -> AppResult<ConditionalWrite> {
        let payload = self.media_resource_payload(resource).await?;
        let unchanged = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    media_id BIGINT,
                    recorded_at TIMESTAMPTZ,
                    media_url TEXT,
                    availability_id SMALLINT,
                    video tweet.media_video
                )
                JOIN LATERAL (
                    SELECT *
                    FROM tweet.media_resource AS latest
                    WHERE latest.media_id = item.media_id
                      AND latest.recorded_at <= item.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
                WHERE latest.media_url IS NOT DISTINCT FROM item.media_url
                  AND latest.availability_id IS NOT DISTINCT FROM item.availability_id
                  AND to_jsonb(latest.video) IS NOT DISTINCT FROM to_jsonb(item.video)
            )
            "#,
        )
        .bind(serde_json::to_value([payload])?)
        .fetch_one(self.pool)
        .await?;

        if unchanged {
            return Ok(ConditionalWrite::SkippedUnchanged);
        }

        match self
            .append_media_resources(std::slice::from_ref(resource))
            .await?
        {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }
}
