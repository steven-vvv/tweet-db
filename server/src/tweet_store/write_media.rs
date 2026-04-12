use super::*;

impl<'a> TweetStore<'a> {
    pub async fn insert_media(&self, media: &[Media]) -> AppResult<u64> {
        if media.is_empty() {
            return Ok(0);
        }

        self.preload_media_dicts(media).await?;
        let mut payloads = Vec::with_capacity(media.len());
        for item in media {
            payloads.push(self.media_payload(item).await?);
        }

        let result = sqlx::query(
            r#"
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
                item.id,
                item.media_type,
                item.alt_text,
                item.grok_post_id,
                item.geometry,
                item.size_variants,
                COALESCE(item.tagged_users, ARRAY[]::tweet.media_tag[]),
                COALESCE(item.sensitivity_warning_ids, ARRAY[]::SMALLINT[]),
                item.origin_tweet_id,
                item.origin_user_id,
                item.details
            FROM jsonb_to_recordset($1::jsonb) AS item(
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
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_media(&self, media: &[Media]) -> AppResult<u64> {
        if media.is_empty() {
            return Ok(0);
        }

        self.preload_media_dicts(media).await?;
        let mut payloads = Vec::with_capacity(media.len());
        for item in media {
            payloads.push(self.media_payload(item).await?);
        }

        let result = sqlx::query(
            r#"
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
                item.id,
                item.media_type,
                item.alt_text,
                item.grok_post_id,
                item.geometry,
                item.size_variants,
                COALESCE(item.tagged_users, ARRAY[]::tweet.media_tag[]),
                COALESCE(item.sensitivity_warning_ids, ARRAY[]::SMALLINT[]),
                item.origin_tweet_id,
                item.origin_user_id,
                item.details
            FROM jsonb_to_recordset($1::jsonb) AS item(
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
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_media_changed(&self, media: &[Media]) -> AppResult<HashSet<i64>> {
        if media.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_media_dicts(media).await?;
        let mut payloads = Vec::with_capacity(media.len());
        for item in media {
            payloads.push(self.media_payload(item).await?);
        }

        let rows = sqlx::query(
            r#"
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
                FROM jsonb_to_recordset($1::jsonb) AS item(
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
            SELECT id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }

    pub async fn append_media_resources(&self, resources: &[MediaResource]) -> AppResult<u64> {
        if resources.is_empty() {
            return Ok(0);
        }

        self.preload_media_resource_dicts(resources).await?;
        let mut payloads = Vec::with_capacity(resources.len());
        for resource in resources {
            payloads.push(self.media_resource_payload(resource).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.media_resource (
                media_id,
                recorded_at,
                media_url,
                availability_id,
                video
            )
            SELECT
                item.media_id,
                item.recorded_at,
                item.media_url,
                item.availability_id,
                item.video
            FROM jsonb_to_recordset($1::jsonb) AS item(
                media_id BIGINT,
                recorded_at TIMESTAMPTZ,
                media_url TEXT,
                availability_id SMALLINT,
                video tweet.media_video
            )
            ON CONFLICT (media_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_user_snapshot_if_changed(
        &self,
        snapshot: &UserSnapshot,
    ) -> AppResult<ConditionalWrite> {
        let payload = self.user_snapshot_payload(snapshot).await?;
        let unchanged = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
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
                JOIN LATERAL (
                    SELECT *
                    FROM tweet.user_snapshot AS latest
                    WHERE latest.user_id = item.user_id
                      AND latest.recorded_at <= item.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
                WHERE latest.display_name IS NOT DISTINCT FROM item.display_name
                  AND latest.user_name IS NOT DISTINCT FROM item.user_name
                  AND latest.avatar_url IS NOT DISTINCT FROM item.avatar_url
                  AND latest.uses_default_avatar IS NOT DISTINCT FROM item.uses_default_avatar
                  AND latest.avatar_shape_id IS NOT DISTINCT FROM item.avatar_shape_id
                  AND latest.banner_url IS NOT DISTINCT FROM item.banner_url
                  AND latest.location IS NOT DISTINCT FROM item.location
                  AND to_jsonb(latest.bio) IS NOT DISTINCT FROM to_jsonb(item.bio)
                  AND to_jsonb(latest.profile_links) IS NOT DISTINCT FROM to_jsonb(COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]))
                  AND to_jsonb(latest.identity) IS NOT DISTINCT FROM to_jsonb(item.identity)
                  AND to_jsonb(latest.features) IS NOT DISTINCT FROM to_jsonb(item.features)
                  AND to_jsonb(latest.professional) IS NOT DISTINCT FROM to_jsonb(item.professional)
                  AND latest.pinned_tweet_ids IS NOT DISTINCT FROM COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[])
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
            .append_user_snapshots(std::slice::from_ref(snapshot))
            .await?
        {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }

    pub async fn append_media_resources_if_changed_many(
        &self,
        resources: &[MediaResource],
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if resources.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_media_resource_dicts(resources).await?;
        let mut payloads = Vec::with_capacity(resources.len());
        for resource in resources {
            payloads.push(self.media_resource_payload(resource).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.media_id, item.recorded_at)
                    item.media_id,
                    item.recorded_at,
                    item.media_url,
                    item.availability_id,
                    item.video
                FROM jsonb_to_recordset($1::jsonb) AS item(
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
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("media_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
    }

    pub async fn replace_tweet_media_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMediaRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_media_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_media_ref (tweet_id, media_id, display_order)
            SELECT item.tweet_id, item.media_id, item.display_order
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                media_id BIGINT,
                display_order SMALLINT
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_mention_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMentionRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_mention_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_mention_ref (tweet_id, user_id)
            SELECT item.tweet_id, item.user_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                user_id BIGINT
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_hashtag_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetHashtagRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_hashtag_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_hashtag_ref (tweet_id, hashtag_id)
            SELECT item.tweet_id, item.hashtag_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                hashtag_id INTEGER
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_symbol_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetSymbolRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_symbol_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_symbol_ref (tweet_id, symbol_id)
            SELECT item.tweet_id, item.symbol_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                symbol_id INTEGER
            )
            "#,
        )
        .await
    }
}
