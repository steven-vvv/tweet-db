use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_tweet_policies(&self, policies: &[TweetPolicy]) -> AppResult<u64> {
        if policies.is_empty() {
            return Ok(0);
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let result = sqlx::query(
            r#"
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
                item.tweet_id,
                item.reply_policy_id,
                item.followers_only,
                item.is_possibly_sensitive,
                COALESCE(item.available_action_ids, ARRAY[]::SMALLINT[]),
                item.is_media_visibility_restricted,
                item.paid_promotion
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                reply_policy_id SMALLINT,
                followers_only BOOLEAN,
                is_possibly_sensitive BOOLEAN,
                available_action_ids SMALLINT[],
                is_media_visibility_restricted BOOLEAN,
                paid_promotion BOOLEAN
            )
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
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_policies_changed(
        &self,
        policies: &[TweetPolicy],
    ) -> AppResult<HashSet<i64>> {
        if policies.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.tweet_id)
                    item.tweet_id,
                    item.reply_policy_id,
                    item.followers_only,
                    item.is_possibly_sensitive,
                    COALESCE(item.available_action_ids, ARRAY[]::SMALLINT[]) AS available_action_ids,
                    item.is_media_visibility_restricted,
                    item.paid_promotion
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    tweet_id BIGINT,
                    reply_policy_id SMALLINT,
                    followers_only BOOLEAN,
                    is_possibly_sensitive BOOLEAN,
                    available_action_ids SMALLINT[],
                    is_media_visibility_restricted BOOLEAN,
                    paid_promotion BOOLEAN
                )
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
            SELECT tweet_id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("tweet_id"))
            .collect())
    }

    pub async fn upsert_tweet_policies_write_statuses(
        &self,
        policies: &[TweetPolicy],
    ) -> AppResult<HashMap<i64, ConditionalWrite>> {
        if policies.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let rows = sqlx::query("SELECT tweet_id, status FROM tweet.write_tweet_policies($1)")
            .bind(serde_json::to_value(payloads)?)
            .fetch_all(self.pool)
            .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.get::<i64, _>("tweet_id"),
                    conditional_write_from_db(row.get::<String, _>("status").as_str())?,
                ))
            })
            .collect()
    }
}
