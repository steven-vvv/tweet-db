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

    pub async fn upsert_tweet_policies(&self, policies: &[TweetPolicy]) -> AppResult<u64> {
        if policies.is_empty() {
            return Ok(0);
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.tweet_policy (
                tweet_id,
                reply_policy_id,
                followers_only,
                is_possibly_sensitive,
                available_action_ids,
                is_media_visibility_restricted,
                paid_promotion
            )
            SELECT
                item.tweet_id,
                item.reply_policy_id,
                item.followers_only,
                item.is_possibly_sensitive,
                COALESCE(item.available_action_ids, ARRAY[]::SMALLINT[]),
                item.is_media_visibility_restricted,
                item.paid_promotion
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                reply_policy_id SMALLINT,
                followers_only BOOLEAN,
                is_possibly_sensitive BOOLEAN,
                available_action_ids SMALLINT[],
                is_media_visibility_restricted BOOLEAN,
                paid_promotion BOOLEAN
            )
            ON CONFLICT (tweet_id) DO UPDATE
            SET reply_policy_id = COALESCE(tweet.tweet_policy.reply_policy_id, EXCLUDED.reply_policy_id),
                followers_only = COALESCE(tweet.tweet_policy.followers_only, EXCLUDED.followers_only),
                is_possibly_sensitive = COALESCE(tweet.tweet_policy.is_possibly_sensitive, EXCLUDED.is_possibly_sensitive),
                available_action_ids = CASE
                    WHEN COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
                     AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
                    THEN EXCLUDED.available_action_ids
                    ELSE tweet.tweet_policy.available_action_ids
                END,
                is_media_visibility_restricted = COALESCE(tweet.tweet_policy.is_media_visibility_restricted, EXCLUDED.is_media_visibility_restricted),
                paid_promotion = COALESCE(tweet.tweet_policy.paid_promotion, EXCLUDED.paid_promotion),
                updated_at = NOW()
            WHERE (tweet.tweet_policy.reply_policy_id IS NULL AND EXCLUDED.reply_policy_id IS NOT NULL)
               OR (tweet.tweet_policy.followers_only IS NULL AND EXCLUDED.followers_only IS NOT NULL)
               OR (tweet.tweet_policy.is_possibly_sensitive IS NULL AND EXCLUDED.is_possibly_sensitive IS NOT NULL)
               OR (
                    COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
                AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
               )
               OR (
                    tweet.tweet_policy.is_media_visibility_restricted IS NULL
                AND EXCLUDED.is_media_visibility_restricted IS NOT NULL
               )
               OR (tweet.tweet_policy.paid_promotion IS NULL AND EXCLUDED.paid_promotion IS NOT NULL)
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_policies_changed(
        &self,
        policies: &[TweetPolicy],
    ) -> AppResult<HashSet<i64>> {
        if policies.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.tweet_id)
                    item.tweet_id,
                    item.reply_policy_id,
                    item.followers_only,
                    item.is_possibly_sensitive,
                    COALESCE(item.available_action_ids, ARRAY[]::SMALLINT[]) AS available_action_ids,
                    item.is_media_visibility_restricted,
                    item.paid_promotion
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    tweet_id BIGINT,
                    reply_policy_id SMALLINT,
                    followers_only BOOLEAN,
                    is_possibly_sensitive BOOLEAN,
                    available_action_ids SMALLINT[],
                    is_media_visibility_restricted BOOLEAN,
                    paid_promotion BOOLEAN
                )
            ),
            changed AS (
                INSERT INTO tweet.tweet_policy (
                    tweet_id,
                    reply_policy_id,
                    followers_only,
                    is_possibly_sensitive,
                    available_action_ids,
                    is_media_visibility_restricted,
                    paid_promotion
                )
                SELECT
                    input.tweet_id,
                    input.reply_policy_id,
                    input.followers_only,
                    input.is_possibly_sensitive,
                    input.available_action_ids,
                    input.is_media_visibility_restricted,
                    input.paid_promotion
                FROM input
                ON CONFLICT (tweet_id) DO UPDATE
                SET reply_policy_id = COALESCE(tweet.tweet_policy.reply_policy_id, EXCLUDED.reply_policy_id),
                    followers_only = COALESCE(tweet.tweet_policy.followers_only, EXCLUDED.followers_only),
                    is_possibly_sensitive = COALESCE(tweet.tweet_policy.is_possibly_sensitive, EXCLUDED.is_possibly_sensitive),
                    available_action_ids = CASE
                        WHEN COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
                         AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
                        THEN EXCLUDED.available_action_ids
                        ELSE tweet.tweet_policy.available_action_ids
                    END,
                    is_media_visibility_restricted = COALESCE(tweet.tweet_policy.is_media_visibility_restricted, EXCLUDED.is_media_visibility_restricted),
                    paid_promotion = COALESCE(tweet.tweet_policy.paid_promotion, EXCLUDED.paid_promotion),
                    updated_at = NOW()
                WHERE (tweet.tweet_policy.reply_policy_id IS NULL AND EXCLUDED.reply_policy_id IS NOT NULL)
                   OR (tweet.tweet_policy.followers_only IS NULL AND EXCLUDED.followers_only IS NOT NULL)
                   OR (tweet.tweet_policy.is_possibly_sensitive IS NULL AND EXCLUDED.is_possibly_sensitive IS NOT NULL)
                   OR (
                        COALESCE(cardinality(tweet.tweet_policy.available_action_ids), 0) = 0
                    AND COALESCE(cardinality(EXCLUDED.available_action_ids), 0) > 0
                   )
                   OR (
                        tweet.tweet_policy.is_media_visibility_restricted IS NULL
                    AND EXCLUDED.is_media_visibility_restricted IS NOT NULL
                   )
                   OR (tweet.tweet_policy.paid_promotion IS NULL AND EXCLUDED.paid_promotion IS NOT NULL)
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

    pub async fn upsert_tweet_policies_write_statuses(
        &self,
        policies: &[TweetPolicy],
    ) -> AppResult<HashMap<i64, ConditionalWrite>> {
        if policies.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_tweet_policy_dicts(policies).await?;
        let mut payloads = Vec::with_capacity(policies.len());
        for policy in policies {
            payloads.push(self.tweet_policy_payload(policy).await?);
        }

        let rows = sqlx::query("SELECT tweet_id, status FROM tweet.write_tweet_policies($1)")
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
