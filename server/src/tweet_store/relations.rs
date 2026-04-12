use super::*;

impl<'a> TweetStore<'a> {
    pub async fn sync_tweet_media_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMediaRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        self.sync_relation_refs("tweet.sync_tweet_media_refs", tweet_ids, refs)
            .await
    }

    pub async fn sync_tweet_mention_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMentionRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        self.sync_relation_refs("tweet.sync_tweet_mention_refs", tweet_ids, refs)
            .await
    }

    pub async fn sync_tweet_hashtag_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetHashtagRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        self.sync_relation_refs("tweet.sync_tweet_hashtag_refs", tweet_ids, refs)
            .await
    }

    pub async fn sync_tweet_symbol_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetSymbolRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        self.sync_relation_refs("tweet.sync_tweet_symbol_refs", tweet_ids, refs)
            .await
    }

    async fn sync_relation_refs<T: Serialize>(
        &self,
        function_name: &str,
        tweet_ids: &[i64],
        refs: &[T],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let sql = format!("SELECT tweet_id, status FROM {function_name}($1, $2)");
        let rows = sqlx::query(&sql)
            .bind(tweet_ids)
            .bind(serde_json::to_value(refs)?)
            .fetch_all(self.pool)
            .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.get::<i64, _>("tweet_id"),
                    relation_sync_status_from_db(row.get::<String, _>("status").as_str())?,
                ))
            })
            .collect()
    }
}
