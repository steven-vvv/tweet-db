use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::{
    error::AppResult,
    string_dict::{StringDictCache, StringSemantic},
    tweet_model::{
        AnnotatedText, GeoPoint, Hashtag, HashtagRef, Media, MediaDetails, MediaEntity,
        MediaGeometry, MediaResource, MediaSizeVariant, MediaTag, MediaVideo, MentionEntity,
        ResolvedUrl, Symbol, SymbolRef, TextStyleRange, Tweet, TweetCommunityNote, TweetEdit,
        TweetHashtagRef, TweetMediaRef, TweetMentionRef, TweetPlace, TweetPolicy, TweetStats,
        TweetSymbolRef, TwitterUser, UrlEntity, UserCategory, UserDisclosure, UserFeatures,
        UserIdentity, UserProfessional, UserSnapshot, UserStats, UserVerification, VideoVariant,
    },
};

pub struct TweetStore<'a> {
    pool: &'a PgPool,
    string_dict: &'a StringDictCache,
}

impl<'a> TweetStore<'a> {
    pub fn new(pool: &'a PgPool, string_dict: &'a StringDictCache) -> Self {
        Self { pool, string_dict }
    }

    pub async fn insert_users(&self, users: &[TwitterUser]) -> AppResult<u64> {
        if users.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.twitter_user (id, registered_at)
            SELECT item.id, item.registered_at
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id BIGINT,
                registered_at TIMESTAMPTZ
            )
            ON CONFLICT (id) DO UPDATE
            SET registered_at = EXCLUDED.registered_at,
                updated_at = NOW()
            WHERE tweet.twitter_user.registered_at IS NULL
              AND EXCLUDED.registered_at IS NOT NULL
            "#,
        )
        .bind(serde_json::to_value(users)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_user_snapshots(&self, snapshots: &[UserSnapshot]) -> AppResult<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }

        let mut payloads = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            payloads.push(self.user_snapshot_payload(snapshot).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.user_snapshot (
                user_id,
                recorded_at,
                display_name,
                user_name,
                avatar_url,
                uses_default_avatar,
                avatar_shape_id,
                banner_url,
                location,
                bio,
                profile_links,
                identity,
                features,
                professional,
                pinned_tweet_ids
            )
            SELECT
                item.user_id,
                item.recorded_at,
                item.display_name,
                item.user_name,
                item.avatar_url,
                item.uses_default_avatar,
                item.avatar_shape_id,
                item.banner_url,
                item.location,
                item.bio,
                COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]),
                item.identity,
                item.features,
                item.professional,
                COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[])
            FROM jsonb_to_recordset($1::jsonb) AS item(
                user_id BIGINT,
                recorded_at TIMESTAMPTZ,
                display_name TEXT,
                user_name TEXT,
                avatar_url TEXT,
                uses_default_avatar BOOLEAN,
                avatar_shape_id SMALLINT,
                banner_url TEXT,
                location TEXT,
                bio tweet.annotated_text,
                profile_links tweet.resolved_url[],
                identity tweet.user_identity,
                features tweet.user_features,
                professional tweet.user_professional,
                pinned_tweet_ids BIGINT[]
            )
            ON CONFLICT (user_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_user_stats(&self, stats: &[UserStats]) -> AppResult<u64> {
        if stats.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.user_stats (
                user_id,
                recorded_at,
                followers,
                following,
                likes,
                media_posts,
                tweets,
                listed
            )
            SELECT
                item.user_id,
                item.recorded_at,
                item.followers,
                item.following,
                item.likes,
                item.media_posts,
                item.tweets,
                item.listed
            FROM jsonb_to_recordset($1::jsonb) AS item(
                user_id BIGINT,
                recorded_at TIMESTAMPTZ,
                followers BIGINT,
                following BIGINT,
                likes BIGINT,
                media_posts BIGINT,
                tweets BIGINT,
                listed BIGINT
            )
            ON CONFLICT (user_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(stats)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_user_categories(
        &self,
        categories: &[UserCategory],
    ) -> AppResult<HashMap<i32, i16>> {
        if categories.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            INSERT INTO tweet.user_category (source_category_code, name)
            SELECT item.source_category_code, item.name
            FROM jsonb_to_recordset($1::jsonb) AS item(
                source_category_code INTEGER,
                name TEXT
            )
            ON CONFLICT (source_category_code) DO UPDATE
            SET name = EXCLUDED.name,
                updated_at = NOW()
            RETURNING source_category_code, id
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
            INSERT INTO tweet.hashtag (tag)
            SELECT item.tag
            FROM jsonb_to_recordset($1::jsonb) AS item(tag TEXT)
            ON CONFLICT (tag) DO UPDATE
            SET tag = EXCLUDED.tag
            RETURNING tag, id
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
            INSERT INTO tweet.symbol (symbol, ticker, name)
            SELECT item.symbol, item.ticker, item.name
            FROM jsonb_to_recordset($1::jsonb) AS item(
                symbol TEXT,
                ticker TEXT,
                name TEXT
            )
            ON CONFLICT (symbol) DO UPDATE
            SET ticker = EXCLUDED.ticker,
                name = EXCLUDED.name,
                updated_at = NOW()
            RETURNING symbol, id
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

    pub async fn upsert_tweet_places(&self, places: &[TweetPlace]) -> AppResult<u64> {
        if places.is_empty() {
            return Ok(0);
        }

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
            SET name = EXCLUDED.name,
                full_name = EXCLUDED.full_name,
                country_id = EXCLUDED.country_id,
                country_code_id = EXCLUDED.country_code_id,
                kind_id = EXCLUDED.kind_id,
                boundary = EXCLUDED.boundary,
                updated_at = NOW()
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn insert_tweets(&self, tweets: &[Tweet]) -> AppResult<u64> {
        if tweets.is_empty() {
            return Ok(0);
        }

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
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
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
            SET version_ids = EXCLUDED.version_ids,
                editable_until = EXCLUDED.editable_until,
                remaining_edits = EXCLUDED.remaining_edits,
                updated_at = NOW()
            "#,
        )
        .bind(serde_json::to_value(edits)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_tweet_policies(&self, policies: &[TweetPolicy]) -> AppResult<u64> {
        if policies.is_empty() {
            return Ok(0);
        }

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
            SET reply_policy_id = EXCLUDED.reply_policy_id,
                followers_only = EXCLUDED.followers_only,
                is_possibly_sensitive = EXCLUDED.is_possibly_sensitive,
                available_action_ids = EXCLUDED.available_action_ids,
                is_media_visibility_restricted = EXCLUDED.is_media_visibility_restricted,
                paid_promotion = EXCLUDED.paid_promotion,
                updated_at = NOW()
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
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

    pub async fn upsert_tweet_community_notes(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<u64> {
        if notes.is_empty() {
            return Ok(0);
        }

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
            SET note_id = EXCLUDED.note_id,
                title = EXCLUDED.title,
                short_title = EXCLUDED.short_title,
                subtitle = EXCLUDED.subtitle,
                footer = EXCLUDED.footer,
                destination_url = EXCLUDED.destination_url,
                updated_at = NOW()
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn insert_media(&self, media: &[Media]) -> AppResult<u64> {
        if media.is_empty() {
            return Ok(0);
        }

        let mut payloads = Vec::with_capacity(media.len());
        for item in media {
            payloads.push(self.media_payload(item).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.media (
                id,
                type,
                alt_text,
                grok_post_id,
                geometry,
                size_variants,
                tagged_users,
                sensitivity_warning_ids,
                origin_tweet_id,
                origin_user_id,
                details
            )
            SELECT
                item.id,
                item.media_type,
                item.alt_text,
                item.grok_post_id,
                item.geometry,
                item.size_variants,
                COALESCE(item.tagged_users, ARRAY[]::tweet.media_tag[]),
                COALESCE(item.sensitivity_warning_ids, ARRAY[]::SMALLINT[]),
                item.origin_tweet_id,
                item.origin_user_id,
                item.details
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id BIGINT,
                media_type tweet.media_type_enum,
                alt_text TEXT,
                grok_post_id UUID,
                geometry tweet.media_geometry,
                size_variants tweet.media_size_variants,
                tagged_users tweet.media_tag[],
                sensitivity_warning_ids SMALLINT[],
                origin_tweet_id BIGINT,
                origin_user_id BIGINT,
                details tweet.media_details
            )
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn append_media_resources(&self, resources: &[MediaResource]) -> AppResult<u64> {
        if resources.is_empty() {
            return Ok(0);
        }

        let mut payloads = Vec::with_capacity(resources.len());
        for resource in resources {
            payloads.push(self.media_resource_payload(resource).await?);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet.media_resource (
                media_id,
                recorded_at,
                media_url,
                availability_id,
                video
            )
            SELECT
                item.media_id,
                item.recorded_at,
                item.media_url,
                item.availability_id,
                item.video
            FROM jsonb_to_recordset($1::jsonb) AS item(
                media_id BIGINT,
                recorded_at TIMESTAMPTZ,
                media_url TEXT,
                availability_id SMALLINT,
                video tweet.media_video
            )
            ON CONFLICT (media_id, recorded_at) DO NOTHING
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn replace_tweet_media_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMediaRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_media_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_media_ref (tweet_id, media_id, display_order)
            SELECT item.tweet_id, item.media_id, item.display_order
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                media_id BIGINT,
                display_order SMALLINT
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_mention_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMentionRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_mention_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_mention_ref (tweet_id, user_id)
            SELECT item.tweet_id, item.user_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                user_id BIGINT
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_hashtag_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetHashtagRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_hashtag_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_hashtag_ref (tweet_id, hashtag_id)
            SELECT item.tweet_id, item.hashtag_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                hashtag_id INTEGER
            )
            "#,
        )
        .await
    }

    pub async fn replace_tweet_symbol_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetSymbolRef],
    ) -> AppResult<()> {
        self.replace_tweet_relations(
            tweet_ids,
            refs,
            "DELETE FROM tweet.tweet_symbol_ref WHERE tweet_id = ANY($1)",
            r#"
            INSERT INTO tweet.tweet_symbol_ref (tweet_id, symbol_id)
            SELECT item.tweet_id, item.symbol_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                tweet_id BIGINT,
                symbol_id INTEGER
            )
            "#,
        )
        .await
    }

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

    async fn replace_tweet_relations<T: Serialize>(
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

    async fn annotated_text_payload(
        &self,
        value: &AnnotatedText,
    ) -> AppResult<AnnotatedTextPayload> {
        let mut styles = Vec::with_capacity(value.styles.len());
        for style in &value.styles {
            styles.push(self.text_style_range_payload(style).await?);
        }

        Ok(AnnotatedTextPayload {
            body: value.body.clone(),
            display_range_start: value.display_range_start,
            display_range_end: value.display_range_end,
            hashtags: value.hashtags.clone(),
            symbols: value.symbols.clone(),
            urls: value.urls.clone(),
            mentions: value.mentions.clone(),
            media_refs: value.media_refs.clone(),
            styles,
        })
    }

    async fn text_style_range_payload(
        &self,
        value: &TextStyleRange,
    ) -> AppResult<TextStyleRangePayload> {
        Ok(TextStyleRangePayload {
            range_start: value.range_start,
            range_end: value.range_end,
            style_ids: self
                .string_dict
                .ensure_ids(self.pool, StringSemantic::TweetTextStyleName, &value.styles)
                .await?,
        })
    }

    async fn user_snapshot_payload(&self, value: &UserSnapshot) -> AppResult<UserSnapshotPayload> {
        Ok(UserSnapshotPayload {
            user_id: value.user_id,
            recorded_at: value.recorded_at,
            display_name: value.display_name.clone(),
            user_name: value.user_name.clone(),
            avatar_url: value.avatar_url.clone(),
            uses_default_avatar: value.uses_default_avatar,
            avatar_shape_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserAvatarShape,
                    value.avatar_shape.as_deref(),
                )
                .await?,
            banner_url: value.banner_url.clone(),
            location: value.location.clone(),
            bio: match value.bio.as_ref() {
                Some(bio) => Some(self.annotated_text_payload(bio).await?),
                None => None,
            },
            profile_links: value.profile_links.clone(),
            identity: match value.identity.as_ref() {
                Some(identity) => Some(self.user_identity_payload(identity).await?),
                None => None,
            },
            features: value.features.clone(),
            professional: match value.professional.as_ref() {
                Some(professional) => Some(self.user_professional_payload(professional).await?),
                None => None,
            },
            pinned_tweet_ids: value.pinned_tweet_ids.clone(),
        })
    }

    async fn user_identity_payload(&self, value: &UserIdentity) -> AppResult<UserIdentityPayload> {
        Ok(UserIdentityPayload {
            verification: match value.verification.as_ref() {
                Some(verification) => Some(self.user_verification_payload(verification).await?),
                None => None,
            },
            disclosure: match value.disclosure.as_ref() {
                Some(disclosure) => Some(self.user_disclosure_payload(disclosure).await?),
                None => None,
            },
            parody_label_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserParodyLabel,
                    value.parody_label.as_deref(),
                )
                .await?,
            has_completed_new_account_review: value.has_completed_new_account_review,
            is_possibly_sensitive: value.is_possibly_sensitive,
        })
    }

    async fn user_verification_payload(
        &self,
        value: &UserVerification,
    ) -> AppResult<UserVerificationPayload> {
        Ok(UserVerificationPayload {
            is_blue_verified: value.is_blue_verified,
            verified_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserVerificationType,
                    value.verified_type.as_deref(),
                )
                .await?,
        })
    }

    async fn user_disclosure_payload(
        &self,
        value: &UserDisclosure,
    ) -> AppResult<UserDisclosurePayload> {
        Ok(UserDisclosurePayload {
            relation_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserDisclosureRelation,
                    value.relation.as_deref(),
                )
                .await?,
            subject_id: value.subject_id,
            subject_handle: value.subject_handle.clone(),
            subject_name: value.subject_name.clone(),
            subject_url: value.subject_url.clone(),
        })
    }

    async fn user_professional_payload(
        &self,
        value: &UserProfessional,
    ) -> AppResult<UserProfessionalPayload> {
        Ok(UserProfessionalPayload {
            professional_id: value.professional_id,
            professional_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserProfessionalType,
                    value.professional_type.as_deref(),
                )
                .await?,
            category_ids: value.category_ids.clone(),
        })
    }

    async fn tweet_place_payload(&self, value: &TweetPlace) -> AppResult<TweetPlacePayload> {
        Ok(TweetPlacePayload {
            id: value.id.clone(),
            name: value.name.clone(),
            full_name: value.full_name.clone(),
            country_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetCountryName,
                    value.country.as_deref(),
                )
                .await?,
            country_code_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetCountryCode,
                    value.country_code.as_deref(),
                )
                .await?,
            kind_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetPlaceKind,
                    value.kind.as_deref(),
                )
                .await?,
            boundary: value.boundary.clone(),
        })
    }

    async fn tweet_payload(&self, value: &Tweet) -> AppResult<TweetPayload> {
        Ok(TweetPayload {
            id: value.id,
            published_at: value.published_at,
            source_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetSource,
                    value.source.as_deref(),
                )
                .await?,
            author_id: value.author_id,
            place_id: value.place_id.clone(),
            legacy_text: self.annotated_text_payload(&value.legacy_text).await?,
            note_id: value.note_id.clone(),
            note_text: match value.note_text.as_ref() {
                Some(note_text) => Some(self.annotated_text_payload(note_text).await?),
                None => None,
            },
            language_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetLanguageCode,
                    value.language.as_deref(),
                )
                .await?,
            conversation_id: value.conversation_id,
            reply_to_tweet_id: value.reply_to_tweet_id,
            reply_to_user_id: value.reply_to_user_id,
            quote_tweet_id: value.quote_tweet_id,
            quote_permalink: value.quote_permalink.clone(),
            repost_id: value.repost_id,
        })
    }

    async fn tweet_policy_payload(&self, value: &TweetPolicy) -> AppResult<TweetPolicyPayload> {
        Ok(TweetPolicyPayload {
            tweet_id: value.tweet_id,
            reply_policy_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetReplyPolicyCode,
                    value.reply_policy.as_deref(),
                )
                .await?,
            followers_only: value.followers_only,
            is_possibly_sensitive: value.is_possibly_sensitive,
            available_action_ids: self
                .string_dict
                .ensure_ids(
                    self.pool,
                    StringSemantic::TweetActionCode,
                    &value.available_actions,
                )
                .await?,
            is_media_visibility_restricted: value.is_media_visibility_restricted,
            paid_promotion: value.paid_promotion,
        })
    }

    async fn tweet_community_note_payload(
        &self,
        value: &TweetCommunityNote,
    ) -> AppResult<TweetCommunityNotePayload> {
        Ok(TweetCommunityNotePayload {
            tweet_id: value.tweet_id,
            note_id: value.note_id,
            title: value.title.clone(),
            short_title: value.short_title.clone(),
            subtitle: match value.subtitle.as_ref() {
                Some(subtitle) => Some(self.annotated_text_payload(subtitle).await?),
                None => None,
            },
            footer: match value.footer.as_ref() {
                Some(footer) => Some(self.annotated_text_payload(footer).await?),
                None => None,
            },
            destination_url: value.destination_url.clone(),
        })
    }

    async fn media_payload(&self, value: &Media) -> AppResult<MediaPayload> {
        Ok(MediaPayload {
            id: value.id,
            media_type: value.media_type.as_db_str().to_owned(),
            alt_text: value.alt_text.clone(),
            grok_post_id: value.grok_post_id,
            geometry: value.geometry.clone(),
            size_variants: match value.size_variants.as_ref() {
                Some(size_variants) => Some(self.media_size_variants_payload(size_variants).await?),
                None => None,
            },
            tagged_users: self.media_tag_payloads(&value.tagged_users).await?,
            sensitivity_warning_ids: self
                .string_dict
                .ensure_ids(
                    self.pool,
                    StringSemantic::TweetMediaSensitivityCode,
                    &value.sensitivity_warnings,
                )
                .await?,
            origin_tweet_id: value.origin_tweet_id,
            origin_user_id: value.origin_user_id,
            details: value.details.clone(),
        })
    }

    async fn media_size_variants_payload(
        &self,
        value: &crate::tweet_model::MediaSizeVariants,
    ) -> AppResult<MediaSizeVariantsPayload> {
        Ok(MediaSizeVariantsPayload {
            large: self
                .optional_media_size_variant_payload(value.large.as_ref())
                .await?,
            medium: self
                .optional_media_size_variant_payload(value.medium.as_ref())
                .await?,
            small: self
                .optional_media_size_variant_payload(value.small.as_ref())
                .await?,
            thumb: self
                .optional_media_size_variant_payload(value.thumb.as_ref())
                .await?,
        })
    }

    async fn optional_media_size_variant_payload(
        &self,
        value: Option<&MediaSizeVariant>,
    ) -> AppResult<Option<MediaSizeVariantPayload>> {
        match value {
            Some(value) => Ok(Some(MediaSizeVariantPayload {
                w: value.w,
                h: value.h,
                resize_mode_id: self
                    .string_dict
                    .ensure_id(
                        self.pool,
                        StringSemantic::TweetMediaResizeMode,
                        Some(&value.resize_mode),
                    )
                    .await?,
            })),
            None => Ok(None),
        }
    }

    async fn media_tag_payloads(&self, value: &[MediaTag]) -> AppResult<Vec<MediaTagPayload>> {
        let mut tags = Vec::with_capacity(value.len());
        for tag in value {
            tags.push(MediaTagPayload {
                user_id: tag.user_id,
                kind_id: self
                    .string_dict
                    .ensure_id(
                        self.pool,
                        StringSemantic::TweetMediaTagKind,
                        tag.kind.as_deref(),
                    )
                    .await?,
            });
        }
        Ok(tags)
    }

    async fn media_resource_payload(
        &self,
        value: &MediaResource,
    ) -> AppResult<MediaResourcePayload> {
        Ok(MediaResourcePayload {
            media_id: value.media_id,
            recorded_at: value.recorded_at,
            media_url: value.media_url.clone(),
            availability_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetMediaAvailabilityStatus,
                    value.availability.as_deref(),
                )
                .await?,
            video: match value.video.as_ref() {
                Some(video) => Some(self.media_video_payload(video).await?),
                None => None,
            },
        })
    }

    async fn media_video_payload(&self, value: &MediaVideo) -> AppResult<MediaVideoPayload> {
        let mut variants = Vec::with_capacity(value.variants.len());
        for variant in &value.variants {
            variants.push(self.video_variant_payload(variant).await?);
        }

        Ok(MediaVideoPayload {
            aspect_ratio_w: value.aspect_ratio_w,
            aspect_ratio_h: value.aspect_ratio_h,
            duration_ms: value.duration_ms,
            variants,
        })
    }

    async fn video_variant_payload(&self, value: &VideoVariant) -> AppResult<VideoVariantPayload> {
        Ok(VideoVariantPayload {
            content_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetVideoContentType,
                    Some(&value.content_type),
                )
                .await?,
            bitrate: value.bitrate,
            url: value.url.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
struct TextStyleRangePayload {
    range_start: i32,
    range_end: i32,
    style_ids: Vec<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct AnnotatedTextPayload {
    body: String,
    display_range_start: Option<i32>,
    display_range_end: Option<i32>,
    hashtags: Vec<HashtagRef>,
    symbols: Vec<SymbolRef>,
    urls: Vec<UrlEntity>,
    mentions: Vec<MentionEntity>,
    media_refs: Vec<MediaEntity>,
    styles: Vec<TextStyleRangePayload>,
}

#[derive(Debug, Clone, Serialize)]
struct UserVerificationPayload {
    is_blue_verified: Option<bool>,
    verified_type_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct UserDisclosurePayload {
    relation_id: Option<i16>,
    subject_id: Option<i64>,
    subject_handle: Option<String>,
    subject_name: Option<String>,
    subject_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UserIdentityPayload {
    verification: Option<UserVerificationPayload>,
    disclosure: Option<UserDisclosurePayload>,
    parody_label_id: Option<i16>,
    has_completed_new_account_review: Option<bool>,
    is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct UserProfessionalPayload {
    professional_id: Option<i64>,
    professional_type_id: Option<i16>,
    category_ids: Vec<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct UserSnapshotPayload {
    user_id: i64,
    recorded_at: time::OffsetDateTime,
    display_name: String,
    user_name: String,
    avatar_url: Option<String>,
    uses_default_avatar: Option<bool>,
    avatar_shape_id: Option<i16>,
    banner_url: Option<String>,
    location: Option<String>,
    bio: Option<AnnotatedTextPayload>,
    profile_links: Vec<ResolvedUrl>,
    identity: Option<UserIdentityPayload>,
    features: Option<UserFeatures>,
    professional: Option<UserProfessionalPayload>,
    pinned_tweet_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct TweetPlacePayload {
    id: String,
    name: Option<String>,
    full_name: Option<String>,
    country_id: Option<i16>,
    country_code_id: Option<i16>,
    kind_id: Option<i16>,
    boundary: Option<Vec<GeoPoint>>,
}

#[derive(Debug, Clone, Serialize)]
struct TweetPayload {
    id: i64,
    published_at: time::OffsetDateTime,
    source_id: Option<i16>,
    author_id: i64,
    place_id: Option<String>,
    legacy_text: AnnotatedTextPayload,
    note_id: Option<String>,
    note_text: Option<AnnotatedTextPayload>,
    language_id: Option<i16>,
    conversation_id: i64,
    reply_to_tweet_id: Option<i64>,
    reply_to_user_id: Option<i64>,
    quote_tweet_id: Option<i64>,
    quote_permalink: Option<ResolvedUrl>,
    repost_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct TweetPolicyPayload {
    tweet_id: i64,
    reply_policy_id: Option<i16>,
    followers_only: Option<bool>,
    is_possibly_sensitive: Option<bool>,
    available_action_ids: Vec<i16>,
    is_media_visibility_restricted: Option<bool>,
    paid_promotion: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct TweetCommunityNotePayload {
    tweet_id: i64,
    note_id: Option<i64>,
    title: Option<String>,
    short_title: Option<String>,
    subtitle: Option<AnnotatedTextPayload>,
    footer: Option<AnnotatedTextPayload>,
    destination_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaSizeVariantPayload {
    w: i32,
    h: i32,
    resize_mode_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaSizeVariantsPayload {
    large: Option<MediaSizeVariantPayload>,
    medium: Option<MediaSizeVariantPayload>,
    small: Option<MediaSizeVariantPayload>,
    thumb: Option<MediaSizeVariantPayload>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaTagPayload {
    user_id: Option<i64>,
    kind_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaPayload {
    id: i64,
    media_type: String,
    alt_text: Option<String>,
    grok_post_id: Option<uuid::Uuid>,
    geometry: Option<MediaGeometry>,
    size_variants: Option<MediaSizeVariantsPayload>,
    tagged_users: Vec<MediaTagPayload>,
    sensitivity_warning_ids: Vec<i16>,
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
    details: Option<MediaDetails>,
}

#[derive(Debug, Clone, Serialize)]
struct VideoVariantPayload {
    content_type_id: Option<i16>,
    bitrate: Option<i32>,
    url: String,
}

#[derive(Debug, Clone, Serialize)]
struct MediaVideoPayload {
    aspect_ratio_w: Option<i32>,
    aspect_ratio_h: Option<i32>,
    duration_ms: Option<i64>,
    variants: Vec<VideoVariantPayload>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaResourcePayload {
    media_id: i64,
    recorded_at: time::OffsetDateTime,
    media_url: Option<String>,
    availability_id: Option<i16>,
    video: Option<MediaVideoPayload>,
}
