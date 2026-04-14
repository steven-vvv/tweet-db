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

        let rows = sqlx::query("SELECT id FROM tweet.write_twitter_users($1)")
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
        self.append_user_stats_if_changed_many(std::slice::from_ref(stats), min_interval_seconds)
            .await?
            .remove(&(stats.user_id, stats.recorded_at))
            .ok_or_else(|| AppError::upstream("missing user stats write status"))
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
            "SELECT user_id, recorded_at, status FROM tweet.append_user_stats_if_changed($1, $2)",
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
            "SELECT user_id, recorded_at, status FROM tweet.append_user_snapshots_if_changed($1)",
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
        self.append_media_resources_if_changed_many(std::slice::from_ref(resource))
            .await?
            .remove(&(resource.media_id, resource.recorded_at))
            .ok_or_else(|| AppError::upstream("missing media resource write status"))
    }
}
