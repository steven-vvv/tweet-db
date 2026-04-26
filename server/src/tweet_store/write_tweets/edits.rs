use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_tweet_edits(&self, edits: &[TweetEdit]) -> AppResult<u64> {
        if edits.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.tweet_edit (tweet_id, version_ids, editable_until, remaining_edits)
            SELECT
                item.tweet_id,
                COALESCE(item.version_ids, ARRAY[]::BIGINT[]),
                item.editable_until,
                item.remaining_edits
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                version_ids BIGINT[],
                editable_until TIMESTAMPTZ,
                remaining_edits INTEGER
            )
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
            "#,
        )
        .bind(serde_json::to_value(edits)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_edits_changed(&self, edits: &[TweetEdit]) -> AppResult<HashSet<i64>> {
        if edits.is_empty() {
            return Ok(HashSet::new());
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.tweet_id)
                    item.tweet_id,
                    COALESCE(item.version_ids, ARRAY[]::BIGINT[]) AS version_ids,
                    item.editable_until,
                    item.remaining_edits
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    tweet_id BIGINT,
                    version_ids BIGINT[],
                    editable_until TIMESTAMPTZ,
                    remaining_edits INTEGER
                )
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
            SELECT tweet_id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(edits)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("tweet_id"))
            .collect())
    }

    pub async fn upsert_tweet_edits_write_statuses(
        &self,
        edits: &[TweetEdit],
    ) -> AppResult<HashMap<i64, ConditionalWrite>> {
        if edits.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query("SELECT tweet_id, status FROM tweet.write_tweet_edits($1)")
            .bind(serde_json::to_value(edits)?)
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
