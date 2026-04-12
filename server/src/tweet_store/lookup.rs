use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_user_categories(
        &self,
        categories: &[UserCategory],
    ) -> AppResult<HashMap<i32, i16>> {
        if categories.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT item.source_category_code, item.name
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    source_category_code INTEGER,
                    name TEXT
                )
            ),
            upserted AS (
                INSERT INTO tweet.user_category (source_category_code, name)
                SELECT input.source_category_code, input.name
                FROM input
                ON CONFLICT (source_category_code) DO UPDATE
                SET name = EXCLUDED.name,
                    updated_at = NOW()
                WHERE tweet.user_category.name = ''
                  AND EXCLUDED.name <> ''
                RETURNING source_category_code, id
            )
            SELECT source_category_code, id
            FROM upserted
            UNION
            SELECT existing.source_category_code, existing.id
            FROM tweet.user_category AS existing
            JOIN input ON input.source_category_code = existing.source_category_code
            "#,
        )
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

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT item.tag
                FROM jsonb_to_recordset($1::jsonb) AS item(tag TEXT)
            ),
            inserted AS (
                INSERT INTO tweet.hashtag (tag)
                SELECT input.tag
                FROM input
                ON CONFLICT (tag) DO NOTHING
                RETURNING tag, id
            )
            SELECT tag, id
            FROM inserted
            UNION
            SELECT existing.tag, existing.id
            FROM tweet.hashtag AS existing
            JOIN input ON input.tag = existing.tag
            "#,
        )
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

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT item.symbol, item.ticker, item.name
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    symbol TEXT,
                    ticker TEXT,
                    name TEXT
                )
            ),
            upserted AS (
                INSERT INTO tweet.symbol (symbol, ticker, name)
                SELECT input.symbol, input.ticker, input.name
                FROM input
                ON CONFLICT (symbol) DO UPDATE
                SET ticker = COALESCE(tweet.symbol.ticker, EXCLUDED.ticker),
                    name = COALESCE(tweet.symbol.name, EXCLUDED.name),
                    updated_at = NOW()
                WHERE (tweet.symbol.ticker IS NULL AND EXCLUDED.ticker IS NOT NULL)
                   OR (tweet.symbol.name IS NULL AND EXCLUDED.name IS NOT NULL)
                RETURNING symbol, id
            )
            SELECT symbol, id
            FROM upserted
            UNION
            SELECT existing.symbol, existing.id
            FROM tweet.symbol AS existing
            JOIN input ON input.symbol = existing.symbol
            "#,
        )
        .bind(serde_json::to_value(symbols)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("symbol"), row.get::<i32, _>("id")))
            .collect())
    }
}
