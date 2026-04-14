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

        let rows = sqlx::query("SELECT id FROM tweet.write_media($1)")
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
        self.append_user_snapshots_if_changed_many(std::slice::from_ref(snapshot))
            .await?
            .remove(&(snapshot.user_id, snapshot.recorded_at))
            .ok_or_else(|| AppError::upstream("missing user snapshot write status"))
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
            "SELECT media_id, recorded_at, status FROM tweet.append_media_resources_if_changed($1)",
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
