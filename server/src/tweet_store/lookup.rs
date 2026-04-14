use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_user_categories(
        &self,
        categories: &[UserCategory],
    ) -> AppResult<HashMap<i32, i16>> {
        if categories.is_empty() {
            return Ok(HashMap::new());
        }

        let rows =
            sqlx::query("SELECT source_category_code, id FROM tweet.resolve_user_categories($1)")
                .bind(serde_json::to_value(categories)?)
                .fetch_all(self.pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<i32, _>("source_category_code"),
                    row.get::<i16, _>("id"),
                )
            })
            .collect())
    }

    pub async fn upsert_hashtags(&self, hashtags: &[Hashtag]) -> AppResult<HashMap<String, i32>> {
        if hashtags.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query("SELECT tag, id FROM tweet.resolve_hashtags($1)")
            .bind(serde_json::to_value(hashtags)?)
            .fetch_all(self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("tag"), row.get::<i32, _>("id")))
            .collect())
    }

    pub async fn upsert_symbols(&self, symbols: &[Symbol]) -> AppResult<HashMap<String, i32>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query("SELECT symbol, id FROM tweet.resolve_symbols($1)")
            .bind(serde_json::to_value(symbols)?)
            .fetch_all(self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("symbol"), row.get::<i32, _>("id")))
            .collect())
    }
}
