use super::*;

impl<'a> TweetStore<'a> {
    pub async fn insert_tweets(&self, tweets: &[Tweet]) -> AppResult<u64> {
        if tweets.is_empty() {
            return Ok(0);
        }

        self.preload_tweet_dicts(tweets).await?;
        let mut payloads = Vec::with_capacity(tweets.len());
        for tweet in tweets {
            payloads.push(self.tweet_payload(tweet).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.tweet (
                id,
                published_at,
                source_id,
                author_id,
                place_id,
                legacy_text,
                note_id,
                note_text,
                language_id,
                conversation_id,
                reply_to_tweet_id,
                reply_to_user_id,
                quote_tweet_id,
                quote_permalink,
                repost_id
            )
            SELECT
                item.id,
                item.published_at,
                item.source_id,
                item.author_id,
                item.place_id,
                item.legacy_text,
                item.note_id,
                item.note_text,
                item.language_id,
                item.conversation_id,
                item.reply_to_tweet_id,
                item.reply_to_user_id,
                item.quote_tweet_id,
                item.quote_permalink,
                item.repost_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id BIGINT,
                published_at TIMESTAMPTZ,
                source_id SMALLINT,
                author_id BIGINT,
                place_id TEXT,
                legacy_text tweet.annotated_text,
                note_id TEXT,
                note_text tweet.annotated_text,
                language_id SMALLINT,
                conversation_id BIGINT,
                reply_to_tweet_id BIGINT,
                reply_to_user_id BIGINT,
                quote_tweet_id BIGINT,
                quote_permalink tweet.resolved_url,
                repost_id BIGINT
            )
            ON CONFLICT (id) DO UPDATE
            SET source_id = COALESCE(tweet.tweet.source_id, EXCLUDED.source_id),
                place_id = COALESCE(tweet.tweet.place_id, EXCLUDED.place_id),
                note_id = COALESCE(tweet.tweet.note_id, EXCLUDED.note_id),
                note_text = COALESCE(tweet.tweet.note_text, EXCLUDED.note_text),
                language_id = COALESCE(tweet.tweet.language_id, EXCLUDED.language_id),
                reply_to_tweet_id = COALESCE(tweet.tweet.reply_to_tweet_id, EXCLUDED.reply_to_tweet_id),
                reply_to_user_id = COALESCE(tweet.tweet.reply_to_user_id, EXCLUDED.reply_to_user_id),
                quote_tweet_id = COALESCE(tweet.tweet.quote_tweet_id, EXCLUDED.quote_tweet_id),
                quote_permalink = COALESCE(tweet.tweet.quote_permalink, EXCLUDED.quote_permalink),
                repost_id = COALESCE(tweet.tweet.repost_id, EXCLUDED.repost_id),
                updated_at = NOW()
            WHERE (tweet.tweet.source_id IS NULL AND EXCLUDED.source_id IS NOT NULL)
               OR (tweet.tweet.place_id IS NULL AND EXCLUDED.place_id IS NOT NULL)
               OR (tweet.tweet.note_id IS NULL AND EXCLUDED.note_id IS NOT NULL)
               OR (tweet.tweet.note_text IS NULL AND EXCLUDED.note_text IS NOT NULL)
               OR (tweet.tweet.language_id IS NULL AND EXCLUDED.language_id IS NOT NULL)
               OR (tweet.tweet.reply_to_tweet_id IS NULL AND EXCLUDED.reply_to_tweet_id IS NOT NULL)
               OR (tweet.tweet.reply_to_user_id IS NULL AND EXCLUDED.reply_to_user_id IS NOT NULL)
               OR (tweet.tweet.quote_tweet_id IS NULL AND EXCLUDED.quote_tweet_id IS NOT NULL)
               OR (tweet.tweet.quote_permalink IS NULL AND EXCLUDED.quote_permalink IS NOT NULL)
               OR (tweet.tweet.repost_id IS NULL AND EXCLUDED.repost_id IS NOT NULL)
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn insert_tweets_changed(&self, tweets: &[Tweet]) -> AppResult<HashSet<i64>> {
        if tweets.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_tweet_dicts(tweets).await?;
        let mut payloads = Vec::with_capacity(tweets.len());
        for tweet in tweets {
            payloads.push(self.tweet_payload(tweet).await?);
        }

        let rows = sqlx::query("SELECT id FROM tweet.write_tweets($1)")
            .bind(serde_json::to_value(payloads)?)
            .fetch_all(self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }
}
