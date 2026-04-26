use super::*;

impl<'a> TweetStore<'a> {
    pub async fn append_tweet_stats(&self, stats: &[TweetStats]) -> AppResult<u64> {
        if stats.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
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
                item.tweet_id,
                item.recorded_at,
                item.views,
                item.replies,
                item.reposts,
                item.quotes,
                item.likes,
                item.bookmarks
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                recorded_at TIMESTAMPTZ,
                views BIGINT,
                replies BIGINT,
                reposts BIGINT,
                quotes BIGINT,
                likes BIGINT,
                bookmarks BIGINT
            )
            ON CONFLICT (tweet_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(stats)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_tweet_stats_if_changed(
        &self,
        stats: &TweetStats,
        min_interval_seconds: i64,
    ) -> AppResult<ConditionalWrite> {
        self.append_tweet_stats_if_changed_many(std::slice::from_ref(stats), min_interval_seconds)
            .await?
            .remove(&(stats.tweet_id, stats.recorded_at))
            .ok_or_else(|| AppError::upstream("missing tweet stats write status"))
    }

    pub async fn append_tweet_stats_if_changed_many(
        &self,
        stats: &[TweetStats],
        min_interval_seconds: i64,
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if stats.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            "SELECT tweet_id, recorded_at, status FROM tweet.append_tweet_stats_if_changed($1, $2)",
        )
        .bind(serde_json::to_value(stats)?)
        .bind(min_interval_seconds)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("tweet_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
    }
}
