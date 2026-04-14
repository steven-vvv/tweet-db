use super::*;

impl<'a> TweetStore<'a> {
    pub async fn sync_tweet_relations(
        &self,
        tweet_ids: &[i64],
        media_refs: &[TweetMediaRef],
        mention_refs: &[TweetMentionRef],
        hashtag_refs: &[TweetHashtagRef],
        symbol_refs: &[TweetSymbolRef],
    ) -> AppResult<HashMap<i64, TweetRelationSyncStatuses>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT tweet_id, media_status, mention_status, hashtag_status, symbol_status
            FROM tweet.sync_tweet_relations($1, $2, $3, $4, $5)
            "#,
        )
        .bind(tweet_ids)
        .bind(serde_json::to_value(media_refs)?)
        .bind(serde_json::to_value(mention_refs)?)
        .bind(serde_json::to_value(hashtag_refs)?)
        .bind(serde_json::to_value(symbol_refs)?)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.get::<i64, _>("tweet_id"),
                    TweetRelationSyncStatuses {
                        media: relation_sync_status_from_db(
                            row.get::<String, _>("media_status").as_str(),
                        )?,
                        mention: relation_sync_status_from_db(
                            row.get::<String, _>("mention_status").as_str(),
                        )?,
                        hashtag: relation_sync_status_from_db(
                            row.get::<String, _>("hashtag_status").as_str(),
                        )?,
                        symbol: relation_sync_status_from_db(
                            row.get::<String, _>("symbol_status").as_str(),
                        )?,
                    },
                ))
            })
            .collect()
    }

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
