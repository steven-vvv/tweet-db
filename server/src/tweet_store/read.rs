use super::*;

impl<'a> TweetStore<'a> {
    pub async fn fetch_latest_user_snapshot_json(&self, user_id: i64) -> AppResult<Option<Value>> {
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(view_row) FROM tweet.v_latest_user_snapshot AS view_row WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn fetch_latest_user_stats_json(&self, user_id: i64) -> AppResult<Option<Value>> {
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(view_row) FROM tweet.v_latest_user_stats AS view_row WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn fetch_latest_tweet_stats_json(&self, tweet_id: i64) -> AppResult<Option<Value>> {
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(view_row) FROM tweet.v_latest_tweet_stats AS view_row WHERE tweet_id = $1",
        )
        .bind(tweet_id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn fetch_latest_media_resource_json(
        &self,
        media_id: i64,
    ) -> AppResult<Option<Value>> {
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(view_row) FROM tweet.v_latest_media_resource AS view_row WHERE media_id = $1",
        )
        .bind(media_id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn fetch_users_json_many(&self, user_ids: &[i64]) -> AppResult<HashMap<i64, Value>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT u.id AS id, to_jsonb(u) AS data
            FROM tweet.twitter_user AS u
            WHERE u.id = ANY($1::BIGINT[])
            "#,
        )
        .bind(user_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_latest_user_snapshots_json_many(
        &self,
        user_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT view_row.user_id AS id, to_jsonb(view_row) AS data
            FROM tweet.v_latest_user_snapshot AS view_row
            WHERE view_row.user_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(user_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_latest_user_stats_json_many(
        &self,
        user_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT view_row.user_id AS id, to_jsonb(view_row) AS data
            FROM tweet.v_latest_user_stats AS view_row
            WHERE view_row.user_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(user_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweets_json_many(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT t.id AS id, to_jsonb(t) AS data
            FROM tweet.tweet AS t
            WHERE t.id = ANY($1::BIGINT[])
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweet_places_json_many(
        &self,
        place_ids: &[String],
    ) -> AppResult<HashMap<String, Value>> {
        if place_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowString>(
            r#"
            SELECT p.id AS id, to_jsonb(p) AS data
            FROM tweet.tweet_place AS p
            WHERE p.id = ANY($1::TEXT[])
            "#,
        )
        .bind(place_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweet_edits_json_many(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT e.tweet_id AS id, to_jsonb(e) AS data
            FROM tweet.tweet_edit AS e
            WHERE e.tweet_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweet_policies_json_many(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT p.tweet_id AS id, to_jsonb(p) AS data
            FROM tweet.tweet_policy AS p
            WHERE p.tweet_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweet_community_notes_json_many(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT n.tweet_id AS id, to_jsonb(n) AS data
            FROM tweet.tweet_community_note AS n
            WHERE n.tweet_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_latest_tweet_stats_json_many(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT view_row.tweet_id AS id, to_jsonb(view_row) AS data
            FROM tweet.v_latest_tweet_stats AS view_row
            WHERE view_row.tweet_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_tweet_media_refs(
        &self,
        tweet_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<TweetMediaRef>>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, TweetMediaRefRow>(
            r#"
            SELECT tweet_id, media_id, display_order
            FROM tweet.tweet_media_ref
            WHERE tweet_id = ANY($1::BIGINT[])
            ORDER BY tweet_id ASC, display_order ASC, media_id ASC
            "#,
        )
        .bind(tweet_ids)
        .fetch_all(self.pool)
        .await?;

        let mut refs = HashMap::<i64, Vec<TweetMediaRef>>::new();
        for row in rows {
            refs.entry(row.tweet_id).or_default().push(TweetMediaRef {
                tweet_id: row.tweet_id,
                media_id: row.media_id,
                display_order: row.display_order,
            });
        }

        Ok(refs)
    }

    pub async fn fetch_media_json_many(&self, media_ids: &[i64]) -> AppResult<HashMap<i64, Value>> {
        if media_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT m.id AS id, to_jsonb(m) AS data
            FROM tweet.media AS m
            WHERE m.id = ANY($1::BIGINT[])
            "#,
        )
        .bind(media_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_latest_media_resource_json_many(
        &self,
        media_ids: &[i64],
    ) -> AppResult<HashMap<i64, Value>> {
        if media_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, JsonRowI64>(
            r#"
            SELECT view_row.media_id AS id, to_jsonb(view_row) AS data
            FROM tweet.v_latest_media_resource AS view_row
            WHERE view_row.media_id = ANY($1::BIGINT[])
            "#,
        )
        .bind(media_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row.data)).collect())
    }

    pub async fn fetch_user_categories(
        &self,
        category_ids: &[i16],
    ) -> AppResult<HashMap<i16, UserCategory>> {
        if category_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, UserCategoryRow>(
            r#"
            SELECT id, source_category_code, name
            FROM tweet.user_category
            WHERE id = ANY($1::SMALLINT[])
            "#,
        )
        .bind(category_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.id,
                    UserCategory {
                        source_category_code: row.source_category_code,
                        name: row.name,
                    },
                )
            })
            .collect())
    }

    pub async fn fetch_hashtags(&self, hashtag_ids: &[i32]) -> AppResult<HashMap<i32, Hashtag>> {
        if hashtag_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, HashtagRow>(
            r#"
            SELECT id, tag
            FROM tweet.hashtag
            WHERE id = ANY($1::INTEGER[])
            "#,
        )
        .bind(hashtag_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.id, Hashtag { tag: row.tag }))
            .collect())
    }

    pub async fn fetch_symbols(&self, symbol_ids: &[i32]) -> AppResult<HashMap<i32, Symbol>> {
        if symbol_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, SymbolRow>(
            r#"
            SELECT id, symbol, ticker, name
            FROM tweet.symbol
            WHERE id = ANY($1::INTEGER[])
            "#,
        )
        .bind(symbol_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.id,
                    Symbol {
                        symbol: row.symbol,
                        ticker: row.ticker,
                        name: row.name,
                    },
                )
            })
            .collect())
    }

    pub(super) async fn replace_tweet_relations<T: Serialize>(
        &self,
        tweet_ids: &[i64],
        refs: &[T],
        delete_sql: &str,
        insert_sql: &str,
    ) -> AppResult<()> {
        if tweet_ids.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query(delete_sql)
            .bind(tweet_ids)
            .execute(&mut *tx)
            .await?;

        if !refs.is_empty() {
            sqlx::query(insert_sql)
                .bind(serde_json::to_value(refs)?)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
