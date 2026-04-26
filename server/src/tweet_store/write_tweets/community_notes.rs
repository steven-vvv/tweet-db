use super::*;

impl<'a> TweetStore<'a> {
    pub async fn upsert_tweet_community_notes(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<u64> {
        if notes.is_empty() {
            return Ok(0);
        }

        self.preload_tweet_community_note_dicts(notes).await?;
        let mut payloads = Vec::with_capacity(notes.len());
        for note in notes {
            payloads.push(self.tweet_community_note_payload(note).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.tweet_community_note (
                tweet_id,
                note_id,
                title,
                short_title,
                subtitle,
                footer,
                destination_url
            )
            SELECT
                item.tweet_id,
                item.note_id,
                item.title,
                item.short_title,
                item.subtitle,
                item.footer,
                item.destination_url
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                note_id BIGINT,
                title TEXT,
                short_title TEXT,
                subtitle tweet.annotated_text,
                footer tweet.annotated_text,
                destination_url TEXT
            )
            ON CONFLICT (tweet_id) DO UPDATE
            SET note_id = COALESCE(tweet.tweet_community_note.note_id, EXCLUDED.note_id),
                title = COALESCE(tweet.tweet_community_note.title, EXCLUDED.title),
                short_title = COALESCE(tweet.tweet_community_note.short_title, EXCLUDED.short_title),
                subtitle = COALESCE(tweet.tweet_community_note.subtitle, EXCLUDED.subtitle),
                footer = COALESCE(tweet.tweet_community_note.footer, EXCLUDED.footer),
                destination_url = COALESCE(tweet.tweet_community_note.destination_url, EXCLUDED.destination_url),
                updated_at = NOW()
            WHERE (tweet.tweet_community_note.note_id IS NULL AND EXCLUDED.note_id IS NOT NULL)
               OR (tweet.tweet_community_note.title IS NULL AND EXCLUDED.title IS NOT NULL)
               OR (tweet.tweet_community_note.short_title IS NULL AND EXCLUDED.short_title IS NOT NULL)
               OR (tweet.tweet_community_note.subtitle IS NULL AND EXCLUDED.subtitle IS NOT NULL)
               OR (tweet.tweet_community_note.footer IS NULL AND EXCLUDED.footer IS NOT NULL)
               OR (
                    tweet.tweet_community_note.destination_url IS NULL
                AND EXCLUDED.destination_url IS NOT NULL
               )
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_community_notes_changed(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<HashSet<i64>> {
        if notes.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_tweet_community_note_dicts(notes).await?;
        let mut payloads = Vec::with_capacity(notes.len());
        for note in notes {
            payloads.push(self.tweet_community_note_payload(note).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.tweet_id)
                    item.tweet_id,
                    item.note_id,
                    item.title,
                    item.short_title,
                    item.subtitle,
                    item.footer,
                    item.destination_url
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    tweet_id BIGINT,
                    note_id BIGINT,
                    title TEXT,
                    short_title TEXT,
                    subtitle tweet.annotated_text,
                    footer tweet.annotated_text,
                    destination_url TEXT
                )
            ),
            changed AS (
                INSERT INTO tweet.tweet_community_note (
                    tweet_id,
                    note_id,
                    title,
                    short_title,
                    subtitle,
                    footer,
                    destination_url
                )
                SELECT
                    input.tweet_id,
                    input.note_id,
                    input.title,
                    input.short_title,
                    input.subtitle,
                    input.footer,
                    input.destination_url
                FROM input
                ON CONFLICT (tweet_id) DO UPDATE
                SET note_id = COALESCE(tweet.tweet_community_note.note_id, EXCLUDED.note_id),
                    title = COALESCE(tweet.tweet_community_note.title, EXCLUDED.title),
                    short_title = COALESCE(tweet.tweet_community_note.short_title, EXCLUDED.short_title),
                    subtitle = COALESCE(tweet.tweet_community_note.subtitle, EXCLUDED.subtitle),
                    footer = COALESCE(tweet.tweet_community_note.footer, EXCLUDED.footer),
                    destination_url = COALESCE(tweet.tweet_community_note.destination_url, EXCLUDED.destination_url),
                    updated_at = NOW()
                WHERE (tweet.tweet_community_note.note_id IS NULL AND EXCLUDED.note_id IS NOT NULL)
                   OR (tweet.tweet_community_note.title IS NULL AND EXCLUDED.title IS NOT NULL)
                   OR (tweet.tweet_community_note.short_title IS NULL AND EXCLUDED.short_title IS NOT NULL)
                   OR (tweet.tweet_community_note.subtitle IS NULL AND EXCLUDED.subtitle IS NOT NULL)
                   OR (tweet.tweet_community_note.footer IS NULL AND EXCLUDED.footer IS NOT NULL)
                   OR (
                        tweet.tweet_community_note.destination_url IS NULL
                    AND EXCLUDED.destination_url IS NOT NULL
                   )
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

    pub async fn upsert_tweet_community_notes_write_statuses(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<HashMap<i64, ConditionalWrite>> {
        if notes.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_tweet_community_note_dicts(notes).await?;
        let mut payloads = Vec::with_capacity(notes.len());
        for note in notes {
            payloads.push(self.tweet_community_note_payload(note).await?);
        }

        let rows =
            sqlx::query("SELECT tweet_id, status FROM tweet.write_tweet_community_notes($1)")
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
