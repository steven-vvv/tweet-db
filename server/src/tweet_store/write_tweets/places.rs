use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_tweet_places(&self, places: &[TweetPlace]) -> AppResult<u64> {
        if places.is_empty() {
            return Ok(0);
        }

        self.preload_tweet_place_dicts(places).await?;
        let mut payloads = Vec::with_capacity(places.len());
        for place in places {
            payloads.push(self.tweet_place_payload(place).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.tweet_place (
                id,
                name,
                full_name,
                country_id,
                country_code_id,
                kind_id,
                boundary
            )
            SELECT
                item.id,
                item.name,
                item.full_name,
                item.country_id,
                item.country_code_id,
                item.kind_id,
                item.boundary
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id TEXT,
                name TEXT,
                full_name TEXT,
                country_id SMALLINT,
                country_code_id SMALLINT,
                kind_id SMALLINT,
                boundary tweet.geo_point[]
            )
            ON CONFLICT (id) DO UPDATE
            SET name = COALESCE(tweet.tweet_place.name, EXCLUDED.name),
                full_name = COALESCE(tweet.tweet_place.full_name, EXCLUDED.full_name),
                country_id = COALESCE(tweet.tweet_place.country_id, EXCLUDED.country_id),
                country_code_id = COALESCE(tweet.tweet_place.country_code_id, EXCLUDED.country_code_id),
                kind_id = COALESCE(tweet.tweet_place.kind_id, EXCLUDED.kind_id),
                boundary = COALESCE(tweet.tweet_place.boundary, EXCLUDED.boundary),
                updated_at = NOW()
            WHERE (tweet.tweet_place.name IS NULL AND EXCLUDED.name IS NOT NULL)
               OR (tweet.tweet_place.full_name IS NULL AND EXCLUDED.full_name IS NOT NULL)
               OR (tweet.tweet_place.country_id IS NULL AND EXCLUDED.country_id IS NOT NULL)
               OR (tweet.tweet_place.country_code_id IS NULL AND EXCLUDED.country_code_id IS NOT NULL)
               OR (tweet.tweet_place.kind_id IS NULL AND EXCLUDED.kind_id IS NOT NULL)
               OR (tweet.tweet_place.boundary IS NULL AND EXCLUDED.boundary IS NOT NULL)
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_places_changed(
        &self,
        places: &[TweetPlace],
    ) -> AppResult<HashSet<String>> {
        if places.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_tweet_place_dicts(places).await?;
        let mut payloads = Vec::with_capacity(places.len());
        for place in places {
            payloads.push(self.tweet_place_payload(place).await?);
        }

        let rows = sqlx::query("SELECT id FROM tweet.write_tweet_places($1)")
            .bind(serde_json::to_value(payloads)?)
            .fetch_all(self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("id"))
            .collect())
    }
}
