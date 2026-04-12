use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::{
    error::{AppError, AppResult},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalWrite {
    Inserted,
    SkippedDuplicate,
    SkippedUnchanged,
    SkippedInterval,
    SkippedMissingParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationSyncStatus {
    Replaced,
    ReplacedFiltered,
    SkippedUnchanged,
    SkippedUnchangedFiltered,
    SkippedMissingTweet,
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

    pub async fn insert_users_changed(&self, users: &[TwitterUser]) -> AppResult<HashSet<i64>> {
        if users.is_empty() {
            return Ok(HashSet::new());
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.id) item.id, item.registered_at
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    id BIGINT,
                    registered_at TIMESTAMPTZ
                )
                ORDER BY item.id, item.registered_at IS NULL
            ),
            changed AS (
                INSERT INTO tweet.twitter_user (id, registered_at)
                SELECT input.id, input.registered_at
                FROM input
                ON CONFLICT (id) DO UPDATE
                SET registered_at = EXCLUDED.registered_at,
                    updated_at = NOW()
                WHERE tweet.twitter_user.registered_at IS NULL
                  AND EXCLUDED.registered_at IS NOT NULL
                RETURNING id
            )
            SELECT id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(users)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }

    pub async fn append_user_snapshots(&self, snapshots: &[UserSnapshot]) -> AppResult<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }

        self.preload_user_snapshot_dicts(snapshots).await?;
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

    pub async fn append_user_stats_if_changed(
        &self,
        stats: &UserStats,
        min_interval_seconds: i64,
    ) -> AppResult<ConditionalWrite> {
        if let Some(previous) = sqlx::query_as::<_, UserStatsRow>(
            r#"
            SELECT recorded_at, followers, following, likes, media_posts, tweets, listed
            FROM tweet.user_stats
            WHERE user_id = $1
              AND recorded_at <= $2
            ORDER BY recorded_at DESC
            LIMIT 1
            "#,
        )
        .bind(stats.user_id)
        .bind(stats.recorded_at)
        .fetch_optional(self.pool)
        .await?
        {
            if previous.same_user_stats(stats) {
                return Ok(ConditionalWrite::SkippedUnchanged);
            }

            if stats.recorded_at - previous.recorded_at
                < time::Duration::seconds(min_interval_seconds)
            {
                return Ok(ConditionalWrite::SkippedInterval);
            }
        }

        match self.append_user_stats(std::slice::from_ref(stats)).await? {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }

    pub async fn append_user_stats_if_changed_many(
        &self,
        stats: &[UserStats],
        min_interval_seconds: i64,
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if stats.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.user_id, item.recorded_at)
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
                ORDER BY item.user_id, item.recorded_at
            ),
            existing_parent AS (
                SELECT input.user_id, input.recorded_at
                FROM input
                JOIN tweet.twitter_user AS parent
                  ON parent.id = input.user_id
            ),
            classified AS (
                SELECT
                    input.*,
                    CASE
                        WHEN existing_parent.user_id IS NULL THEN 'missing_parent'
                        WHEN latest.recorded_at IS NOT NULL
                         AND latest.followers IS NOT DISTINCT FROM input.followers
                         AND latest.following IS NOT DISTINCT FROM input.following
                         AND latest.likes IS NOT DISTINCT FROM input.likes
                         AND latest.media_posts IS NOT DISTINCT FROM input.media_posts
                         AND latest.tweets IS NOT DISTINCT FROM input.tweets
                         AND latest.listed IS NOT DISTINCT FROM input.listed
                            THEN 'unchanged'
                        WHEN latest.recorded_at IS NOT NULL
                         AND input.recorded_at - latest.recorded_at < ($2::DOUBLE PRECISION * INTERVAL '1 second')
                            THEN 'interval'
                        ELSE 'candidate'
                    END AS decision
                FROM input
                LEFT JOIN existing_parent
                  ON existing_parent.user_id = input.user_id
                 AND existing_parent.recorded_at = input.recorded_at
                LEFT JOIN LATERAL (
                    SELECT recorded_at, followers, following, likes, media_posts, tweets, listed
                    FROM tweet.user_stats AS latest
                    WHERE latest.user_id = input.user_id
                      AND latest.recorded_at <= input.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
            ),
            inserted AS (
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
                    user_id,
                    recorded_at,
                    followers,
                    following,
                    likes,
                    media_posts,
                    tweets,
                    listed
                FROM classified
                WHERE decision = 'candidate'
                ON CONFLICT (user_id, recorded_at) DO NOTHING
                RETURNING user_id, recorded_at
            )
            SELECT
                classified.user_id,
                classified.recorded_at,
                CASE
                    WHEN classified.decision = 'missing_parent' THEN 'missing_parent'
                    WHEN inserted.user_id IS NOT NULL THEN 'inserted'
                    WHEN classified.decision = 'unchanged' THEN 'unchanged'
                    WHEN classified.decision = 'interval' THEN 'interval'
                    ELSE 'duplicate'
                END AS status
            FROM classified
            LEFT JOIN inserted
              ON inserted.user_id = classified.user_id
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(stats)?)
        .bind(min_interval_seconds)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("user_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
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

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.id)
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
            ),
            changed AS (
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
                    input.id,
                    input.name,
                    input.full_name,
                    input.country_id,
                    input.country_code_id,
                    input.kind_id,
                    input.boundary
                FROM input
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
                RETURNING id
            )
            SELECT id
            FROM changed
            "#,
        )
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

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.id)
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
            ),
            changed AS (
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
                    input.id,
                    input.published_at,
                    input.source_id,
                    input.author_id,
                    input.place_id,
                    input.legacy_text,
                    input.note_id,
                    input.note_text,
                    input.language_id,
                    input.conversation_id,
                    input.reply_to_tweet_id,
                    input.reply_to_user_id,
                    input.quote_tweet_id,
                    input.quote_permalink,
                    input.repost_id
                FROM input
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
                RETURNING id
            )
            SELECT id
            FROM changed
            "#,
        )
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
                ORDER BY item.tweet_id
            ),
            existing_parent AS (
                SELECT input.tweet_id
                FROM input
                JOIN tweet.tweet AS parent
                  ON parent.id = input.tweet_id
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
                JOIN existing_parent
                  ON existing_parent.tweet_id = input.tweet_id
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
            SELECT
                input.tweet_id,
                CASE
                    WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
                    WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
                    ELSE 'unchanged'
                END AS status
            FROM input
            LEFT JOIN existing_parent
              ON existing_parent.tweet_id = input.tweet_id
            LEFT JOIN changed
              ON changed.tweet_id = input.tweet_id
            "#,
        )
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
                ORDER BY item.tweet_id
            ),
            existing_parent AS (
                SELECT input.tweet_id
                FROM input
                JOIN tweet.tweet AS parent
                  ON parent.id = input.tweet_id
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
                JOIN existing_parent
                  ON existing_parent.tweet_id = input.tweet_id
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
            SELECT
                input.tweet_id,
                CASE
                    WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
                    WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
                    ELSE 'unchanged'
                END AS status
            FROM input
            LEFT JOIN existing_parent
              ON existing_parent.tweet_id = input.tweet_id
            LEFT JOIN changed
              ON changed.tweet_id = input.tweet_id
            "#,
        )
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
        if let Some(previous) = sqlx::query_as::<_, TweetStatsRow>(
            r#"
            SELECT recorded_at, views, replies, reposts, quotes, likes, bookmarks
            FROM tweet.tweet_stats
            WHERE tweet_id = $1
              AND recorded_at <= $2
            ORDER BY recorded_at DESC
            LIMIT 1
            "#,
        )
        .bind(stats.tweet_id)
        .bind(stats.recorded_at)
        .fetch_optional(self.pool)
        .await?
        {
            if previous.same_tweet_stats(stats) {
                return Ok(ConditionalWrite::SkippedUnchanged);
            }

            if stats.recorded_at - previous.recorded_at
                < time::Duration::seconds(min_interval_seconds)
            {
                return Ok(ConditionalWrite::SkippedInterval);
            }
        }

        match self.append_tweet_stats(std::slice::from_ref(stats)).await? {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
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
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.tweet_id, item.recorded_at)
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
                ORDER BY item.tweet_id, item.recorded_at
            ),
            existing_parent AS (
                SELECT input.tweet_id, input.recorded_at
                FROM input
                JOIN tweet.tweet AS parent
                  ON parent.id = input.tweet_id
            ),
            classified AS (
                SELECT
                    input.*,
                    CASE
                        WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
                        WHEN latest.recorded_at IS NOT NULL
                         AND latest.views IS NOT DISTINCT FROM input.views
                         AND latest.replies IS NOT DISTINCT FROM input.replies
                         AND latest.reposts IS NOT DISTINCT FROM input.reposts
                         AND latest.quotes IS NOT DISTINCT FROM input.quotes
                         AND latest.likes IS NOT DISTINCT FROM input.likes
                         AND latest.bookmarks IS NOT DISTINCT FROM input.bookmarks
                            THEN 'unchanged'
                        WHEN latest.recorded_at IS NOT NULL
                         AND input.recorded_at - latest.recorded_at < ($2::DOUBLE PRECISION * INTERVAL '1 second')
                            THEN 'interval'
                        ELSE 'candidate'
                    END AS decision
                FROM input
                LEFT JOIN existing_parent
                  ON existing_parent.tweet_id = input.tweet_id
                 AND existing_parent.recorded_at = input.recorded_at
                LEFT JOIN LATERAL (
                    SELECT recorded_at, views, replies, reposts, quotes, likes, bookmarks
                    FROM tweet.tweet_stats AS latest
                    WHERE latest.tweet_id = input.tweet_id
                      AND latest.recorded_at <= input.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
            ),
            inserted AS (
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
                    tweet_id,
                    recorded_at,
                    views,
                    replies,
                    reposts,
                    quotes,
                    likes,
                    bookmarks
                FROM classified
                WHERE decision = 'candidate'
                ON CONFLICT (tweet_id, recorded_at) DO NOTHING
                RETURNING tweet_id, recorded_at
            )
            SELECT
                classified.tweet_id,
                classified.recorded_at,
                CASE
                    WHEN classified.decision = 'missing_parent' THEN 'missing_parent'
                    WHEN inserted.tweet_id IS NOT NULL THEN 'inserted'
                    WHEN classified.decision = 'unchanged' THEN 'unchanged'
                    WHEN classified.decision = 'interval' THEN 'interval'
                    ELSE 'duplicate'
                END AS status
            FROM classified
            LEFT JOIN inserted
              ON inserted.tweet_id = classified.tweet_id
             AND inserted.recorded_at = classified.recorded_at
            "#,
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
                ORDER BY item.tweet_id
            ),
            existing_parent AS (
                SELECT input.tweet_id
                FROM input
                JOIN tweet.tweet AS parent
                  ON parent.id = input.tweet_id
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
                JOIN existing_parent
                  ON existing_parent.tweet_id = input.tweet_id
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
            SELECT
                input.tweet_id,
                CASE
                    WHEN existing_parent.tweet_id IS NULL THEN 'missing_parent'
                    WHEN changed.tweet_id IS NOT NULL THEN 'inserted'
                    ELSE 'unchanged'
                END AS status
            FROM input
            LEFT JOIN existing_parent
              ON existing_parent.tweet_id = input.tweet_id
            LEFT JOIN changed
              ON changed.tweet_id = input.tweet_id
            "#,
        )
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

    pub async fn insert_media(&self, media: &[Media]) -> AppResult<u64> {
        if media.is_empty() {
            return Ok(0);
        }

        self.preload_media_dicts(media).await?;
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

    pub async fn upsert_media(&self, media: &[Media]) -> AppResult<u64> {
        if media.is_empty() {
            return Ok(0);
        }

        self.preload_media_dicts(media).await?;
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
            ON CONFLICT (id) DO UPDATE
            SET alt_text = COALESCE(tweet.media.alt_text, EXCLUDED.alt_text),
                grok_post_id = COALESCE(tweet.media.grok_post_id, EXCLUDED.grok_post_id),
                geometry = COALESCE(tweet.media.geometry, EXCLUDED.geometry),
                size_variants = COALESCE(tweet.media.size_variants, EXCLUDED.size_variants),
                tagged_users = CASE
                    WHEN COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
                     AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
                    THEN EXCLUDED.tagged_users
                    ELSE tweet.media.tagged_users
                END,
                sensitivity_warning_ids = CASE
                    WHEN COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
                     AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
                    THEN EXCLUDED.sensitivity_warning_ids
                    ELSE tweet.media.sensitivity_warning_ids
                END,
                origin_tweet_id = COALESCE(tweet.media.origin_tweet_id, EXCLUDED.origin_tweet_id),
                origin_user_id = COALESCE(tweet.media.origin_user_id, EXCLUDED.origin_user_id),
                details = COALESCE(tweet.media.details, EXCLUDED.details),
                updated_at = NOW()
            WHERE (tweet.media.alt_text IS NULL AND EXCLUDED.alt_text IS NOT NULL)
               OR (tweet.media.grok_post_id IS NULL AND EXCLUDED.grok_post_id IS NOT NULL)
               OR (tweet.media.geometry IS NULL AND EXCLUDED.geometry IS NOT NULL)
               OR (tweet.media.size_variants IS NULL AND EXCLUDED.size_variants IS NOT NULL)
               OR (
                    COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
                AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
               )
               OR (
                    COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
                AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
               )
               OR (tweet.media.origin_tweet_id IS NULL AND EXCLUDED.origin_tweet_id IS NOT NULL)
               OR (tweet.media.origin_user_id IS NULL AND EXCLUDED.origin_user_id IS NOT NULL)
               OR (tweet.media.details IS NULL AND EXCLUDED.details IS NOT NULL)
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn upsert_media_changed(&self, media: &[Media]) -> AppResult<HashSet<i64>> {
        if media.is_empty() {
            return Ok(HashSet::new());
        }

        self.preload_media_dicts(media).await?;
        let mut payloads = Vec::with_capacity(media.len());
        for item in media {
            payloads.push(self.media_payload(item).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.id)
                    item.id,
                    item.media_type,
                    item.alt_text,
                    item.grok_post_id,
                    item.geometry,
                    item.size_variants,
                    COALESCE(item.tagged_users, ARRAY[]::tweet.media_tag[]) AS tagged_users,
                    COALESCE(item.sensitivity_warning_ids, ARRAY[]::SMALLINT[]) AS sensitivity_warning_ids,
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
            ),
            changed AS (
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
                    input.id,
                    input.media_type,
                    input.alt_text,
                    input.grok_post_id,
                    input.geometry,
                    input.size_variants,
                    input.tagged_users,
                    input.sensitivity_warning_ids,
                    input.origin_tweet_id,
                    input.origin_user_id,
                    input.details
                FROM input
                ON CONFLICT (id) DO UPDATE
                SET alt_text = COALESCE(tweet.media.alt_text, EXCLUDED.alt_text),
                    grok_post_id = COALESCE(tweet.media.grok_post_id, EXCLUDED.grok_post_id),
                    geometry = COALESCE(tweet.media.geometry, EXCLUDED.geometry),
                    size_variants = COALESCE(tweet.media.size_variants, EXCLUDED.size_variants),
                    tagged_users = CASE
                        WHEN COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
                         AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
                        THEN EXCLUDED.tagged_users
                        ELSE tweet.media.tagged_users
                    END,
                    sensitivity_warning_ids = CASE
                        WHEN COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
                         AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
                        THEN EXCLUDED.sensitivity_warning_ids
                        ELSE tweet.media.sensitivity_warning_ids
                    END,
                    origin_tweet_id = COALESCE(tweet.media.origin_tweet_id, EXCLUDED.origin_tweet_id),
                    origin_user_id = COALESCE(tweet.media.origin_user_id, EXCLUDED.origin_user_id),
                    details = COALESCE(tweet.media.details, EXCLUDED.details),
                    updated_at = NOW()
                WHERE (tweet.media.alt_text IS NULL AND EXCLUDED.alt_text IS NOT NULL)
                   OR (tweet.media.grok_post_id IS NULL AND EXCLUDED.grok_post_id IS NOT NULL)
                   OR (tweet.media.geometry IS NULL AND EXCLUDED.geometry IS NOT NULL)
                   OR (tweet.media.size_variants IS NULL AND EXCLUDED.size_variants IS NOT NULL)
                   OR (
                        COALESCE(cardinality(tweet.media.tagged_users), 0) = 0
                    AND COALESCE(cardinality(EXCLUDED.tagged_users), 0) > 0
                   )
                   OR (
                        COALESCE(cardinality(tweet.media.sensitivity_warning_ids), 0) = 0
                    AND COALESCE(cardinality(EXCLUDED.sensitivity_warning_ids), 0) > 0
                   )
                   OR (tweet.media.origin_tweet_id IS NULL AND EXCLUDED.origin_tweet_id IS NOT NULL)
                   OR (tweet.media.origin_user_id IS NULL AND EXCLUDED.origin_user_id IS NOT NULL)
                   OR (tweet.media.details IS NULL AND EXCLUDED.details IS NOT NULL)
                RETURNING id
            )
            SELECT id
            FROM changed
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }

    pub async fn append_media_resources(&self, resources: &[MediaResource]) -> AppResult<u64> {
        if resources.is_empty() {
            return Ok(0);
        }

        self.preload_media_resource_dicts(resources).await?;
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

    pub async fn append_user_snapshot_if_changed(
        &self,
        snapshot: &UserSnapshot,
    ) -> AppResult<ConditionalWrite> {
        let payload = self.user_snapshot_payload(snapshot).await?;
        let unchanged = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
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
                JOIN LATERAL (
                    SELECT *
                    FROM tweet.user_snapshot AS latest
                    WHERE latest.user_id = item.user_id
                      AND latest.recorded_at <= item.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
                WHERE latest.display_name IS NOT DISTINCT FROM item.display_name
                  AND latest.user_name IS NOT DISTINCT FROM item.user_name
                  AND latest.avatar_url IS NOT DISTINCT FROM item.avatar_url
                  AND latest.uses_default_avatar IS NOT DISTINCT FROM item.uses_default_avatar
                  AND latest.avatar_shape_id IS NOT DISTINCT FROM item.avatar_shape_id
                  AND latest.banner_url IS NOT DISTINCT FROM item.banner_url
                  AND latest.location IS NOT DISTINCT FROM item.location
                  AND to_jsonb(latest.bio) IS NOT DISTINCT FROM to_jsonb(item.bio)
                  AND to_jsonb(latest.profile_links) IS NOT DISTINCT FROM to_jsonb(COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]))
                  AND to_jsonb(latest.identity) IS NOT DISTINCT FROM to_jsonb(item.identity)
                  AND to_jsonb(latest.features) IS NOT DISTINCT FROM to_jsonb(item.features)
                  AND to_jsonb(latest.professional) IS NOT DISTINCT FROM to_jsonb(item.professional)
                  AND latest.pinned_tweet_ids IS NOT DISTINCT FROM COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[])
            )
            "#,
        )
        .bind(serde_json::to_value([payload])?)
        .fetch_one(self.pool)
        .await?;

        if unchanged {
            return Ok(ConditionalWrite::SkippedUnchanged);
        }

        match self
            .append_user_snapshots(std::slice::from_ref(snapshot))
            .await?
        {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }

    pub async fn append_user_snapshots_if_changed_many(
        &self,
        snapshots: &[UserSnapshot],
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if snapshots.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_user_snapshot_dicts(snapshots).await?;
        let mut payloads = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            payloads.push(self.user_snapshot_payload(snapshot).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.user_id, item.recorded_at)
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
                    COALESCE(item.profile_links, ARRAY[]::tweet.resolved_url[]) AS profile_links,
                    item.identity,
                    item.features,
                    item.professional,
                    COALESCE(item.pinned_tweet_ids, ARRAY[]::BIGINT[]) AS pinned_tweet_ids
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
                ORDER BY item.user_id, item.recorded_at
            ),
            existing_parent AS (
                SELECT input.user_id, input.recorded_at
                FROM input
                JOIN tweet.twitter_user AS parent
                  ON parent.id = input.user_id
            ),
            classified AS (
                SELECT
                    input.*,
                    (existing_parent.user_id IS NOT NULL) AS has_parent,
                    (
                        existing_parent.user_id IS NOT NULL
                    AND latest.user_id IS NOT NULL
                    AND latest.display_name IS NOT DISTINCT FROM input.display_name
                    AND latest.user_name IS NOT DISTINCT FROM input.user_name
                    AND latest.avatar_url IS NOT DISTINCT FROM input.avatar_url
                    AND latest.uses_default_avatar IS NOT DISTINCT FROM input.uses_default_avatar
                    AND latest.avatar_shape_id IS NOT DISTINCT FROM input.avatar_shape_id
                    AND latest.banner_url IS NOT DISTINCT FROM input.banner_url
                    AND latest.location IS NOT DISTINCT FROM input.location
                    AND to_jsonb(latest.bio) IS NOT DISTINCT FROM to_jsonb(input.bio)
                    AND to_jsonb(latest.profile_links) IS NOT DISTINCT FROM to_jsonb(input.profile_links)
                    AND to_jsonb(latest.identity) IS NOT DISTINCT FROM to_jsonb(input.identity)
                    AND to_jsonb(latest.features) IS NOT DISTINCT FROM to_jsonb(input.features)
                    AND to_jsonb(latest.professional) IS NOT DISTINCT FROM to_jsonb(input.professional)
                    AND latest.pinned_tweet_ids IS NOT DISTINCT FROM input.pinned_tweet_ids
                    ) AS unchanged
                FROM input
                LEFT JOIN existing_parent
                  ON existing_parent.user_id = input.user_id
                 AND existing_parent.recorded_at = input.recorded_at
                LEFT JOIN LATERAL (
                    SELECT *
                    FROM tweet.user_snapshot AS latest
                    WHERE latest.user_id = input.user_id
                      AND latest.recorded_at <= input.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
            ),
            inserted AS (
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
                FROM classified
                WHERE has_parent
                  AND NOT unchanged
                ON CONFLICT (user_id, recorded_at) DO NOTHING
                RETURNING user_id, recorded_at
            )
            SELECT
                classified.user_id,
                classified.recorded_at,
                CASE
                    WHEN NOT classified.has_parent THEN 'missing_parent'
                    WHEN inserted.user_id IS NOT NULL THEN 'inserted'
                    WHEN classified.unchanged THEN 'unchanged'
                    ELSE 'duplicate'
                END AS status
            FROM classified
            LEFT JOIN inserted
              ON inserted.user_id = classified.user_id
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("user_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
    }

    pub async fn append_media_resource_if_changed(
        &self,
        resource: &MediaResource,
    ) -> AppResult<ConditionalWrite> {
        let payload = self.media_resource_payload(resource).await?;
        let unchanged = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM jsonb_to_recordset($1::jsonb) AS item(
                    media_id BIGINT,
                    recorded_at TIMESTAMPTZ,
                    media_url TEXT,
                    availability_id SMALLINT,
                    video tweet.media_video
                )
                JOIN LATERAL (
                    SELECT *
                    FROM tweet.media_resource AS latest
                    WHERE latest.media_id = item.media_id
                      AND latest.recorded_at <= item.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
                WHERE latest.media_url IS NOT DISTINCT FROM item.media_url
                  AND latest.availability_id IS NOT DISTINCT FROM item.availability_id
                  AND to_jsonb(latest.video) IS NOT DISTINCT FROM to_jsonb(item.video)
            )
            "#,
        )
        .bind(serde_json::to_value([payload])?)
        .fetch_one(self.pool)
        .await?;

        if unchanged {
            return Ok(ConditionalWrite::SkippedUnchanged);
        }

        match self
            .append_media_resources(std::slice::from_ref(resource))
            .await?
        {
            0 => Ok(ConditionalWrite::SkippedDuplicate),
            _ => Ok(ConditionalWrite::Inserted),
        }
    }

    pub async fn append_media_resources_if_changed_many(
        &self,
        resources: &[MediaResource],
    ) -> AppResult<HashMap<(i64, time::OffsetDateTime), ConditionalWrite>> {
        if resources.is_empty() {
            return Ok(HashMap::new());
        }

        self.preload_media_resource_dicts(resources).await?;
        let mut payloads = Vec::with_capacity(resources.len());
        for resource in resources {
            payloads.push(self.media_resource_payload(resource).await?);
        }

        let rows = sqlx::query(
            r#"
            WITH input AS (
                SELECT DISTINCT ON (item.media_id, item.recorded_at)
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
                ORDER BY item.media_id, item.recorded_at
            ),
            existing_parent AS (
                SELECT input.media_id, input.recorded_at
                FROM input
                JOIN tweet.media AS parent
                  ON parent.id = input.media_id
            ),
            classified AS (
                SELECT
                    input.*,
                    (existing_parent.media_id IS NOT NULL) AS has_parent,
                    (
                        existing_parent.media_id IS NOT NULL
                    AND latest.media_id IS NOT NULL
                    AND latest.media_url IS NOT DISTINCT FROM input.media_url
                    AND latest.availability_id IS NOT DISTINCT FROM input.availability_id
                    AND to_jsonb(latest.video) IS NOT DISTINCT FROM to_jsonb(input.video)
                    ) AS unchanged
                FROM input
                LEFT JOIN existing_parent
                  ON existing_parent.media_id = input.media_id
                 AND existing_parent.recorded_at = input.recorded_at
                LEFT JOIN LATERAL (
                    SELECT *
                    FROM tweet.media_resource AS latest
                    WHERE latest.media_id = input.media_id
                      AND latest.recorded_at <= input.recorded_at
                    ORDER BY latest.recorded_at DESC
                    LIMIT 1
                ) AS latest ON true
            ),
            inserted AS (
                INSERT INTO tweet.media_resource (
                    media_id,
                    recorded_at,
                    media_url,
                    availability_id,
                    video
                )
                SELECT
                    media_id,
                    recorded_at,
                    media_url,
                    availability_id,
                    video
                FROM classified
                WHERE has_parent
                  AND NOT unchanged
                ON CONFLICT (media_id, recorded_at) DO NOTHING
                RETURNING media_id, recorded_at
            )
            SELECT
                classified.media_id,
                classified.recorded_at,
                CASE
                    WHEN NOT classified.has_parent THEN 'missing_parent'
                    WHEN inserted.media_id IS NOT NULL THEN 'inserted'
                    WHEN classified.unchanged THEN 'unchanged'
                    ELSE 'duplicate'
                END AS status
            FROM classified
            LEFT JOIN inserted
              ON inserted.media_id = classified.media_id
             AND inserted.recorded_at = classified.recorded_at
            "#,
        )
        .bind(serde_json::to_value(payloads)?)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let key = (
                    row.get::<i64, _>("media_id"),
                    row.get::<time::OffsetDateTime, _>("recorded_at"),
                );
                let status = conditional_write_from_db(row.get::<String, _>("status").as_str())?;
                Ok((key, status))
            })
            .collect()
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

    pub async fn sync_tweet_media_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMediaRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH target_tweets AS (
                SELECT DISTINCT UNNEST($1::BIGINT[]) AS tweet_id
            ),
            existing_tweets AS (
                SELECT target_tweets.tweet_id
                FROM target_tweets
                JOIN tweet.tweet AS t
                  ON t.id = target_tweets.tweet_id
            ),
            raw_input AS (
                SELECT DISTINCT ON (item.tweet_id, item.media_id)
                    item.tweet_id,
                    item.media_id,
                    item.display_order
                FROM jsonb_to_recordset($2::jsonb) AS item(
                    tweet_id BIGINT,
                    media_id BIGINT,
                    display_order SMALLINT
                )
                JOIN existing_tweets
                  ON existing_tweets.tweet_id = item.tweet_id
                ORDER BY item.tweet_id, item.media_id, item.display_order
            ),
            input_refs AS (
                SELECT raw_input.tweet_id, raw_input.media_id, raw_input.display_order
                FROM raw_input
                JOIN tweet.media AS m
                  ON m.id = raw_input.media_id
            ),
            requested_counts AS (
                SELECT tweet_id, COUNT(*)::INTEGER AS requested_count
                FROM raw_input
                GROUP BY tweet_id
            ),
            effective_counts AS (
                SELECT tweet_id, COUNT(*)::INTEGER AS effective_count
                FROM input_refs
                GROUP BY tweet_id
            ),
            desired_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(
                            jsonb_build_object(
                                'media_id', input_refs.media_id,
                                'display_order', input_refs.display_order
                            )
                            ORDER BY input_refs.display_order, input_refs.media_id
                        ) FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN input_refs
                  ON input_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            current_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(
                            jsonb_build_object(
                                'media_id', current_refs.media_id,
                                'display_order', current_refs.display_order
                            )
                            ORDER BY current_refs.display_order, current_refs.media_id
                        ) FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN tweet.tweet_media_ref AS current_refs
                  ON current_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            changed_tweets AS (
                SELECT
                    desired_state.tweet_id,
                    COALESCE(requested_counts.requested_count, 0)
                        > COALESCE(effective_counts.effective_count, 0) AS filtered
                FROM desired_state
                JOIN current_state
                  ON current_state.tweet_id = desired_state.tweet_id
                LEFT JOIN requested_counts
                  ON requested_counts.tweet_id = desired_state.tweet_id
                LEFT JOIN effective_counts
                  ON effective_counts.tweet_id = desired_state.tweet_id
                WHERE desired_state.refs IS DISTINCT FROM current_state.refs
            ),
            deleted AS (
                DELETE FROM tweet.tweet_media_ref
                WHERE tweet_id IN (SELECT tweet_id FROM changed_tweets)
            ),
            inserted AS (
                INSERT INTO tweet.tweet_media_ref (tweet_id, media_id, display_order)
                SELECT input_refs.tweet_id, input_refs.media_id, input_refs.display_order
                FROM input_refs
                JOIN changed_tweets
                  ON changed_tweets.tweet_id = input_refs.tweet_id
            )
            SELECT
                target_tweets.tweet_id,
                CASE
                    WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
                    WHEN changed_tweets.tweet_id IS NOT NULL AND changed_tweets.filtered THEN 'replaced_filtered'
                    WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
                    WHEN COALESCE(requested_counts.requested_count, 0)
                       > COALESCE(effective_counts.effective_count, 0) THEN 'unchanged_filtered'
                    ELSE 'unchanged'
                END AS status
            FROM target_tweets
            LEFT JOIN existing_tweets
              ON existing_tweets.tweet_id = target_tweets.tweet_id
            LEFT JOIN changed_tweets
              ON changed_tweets.tweet_id = target_tweets.tweet_id
            LEFT JOIN requested_counts
              ON requested_counts.tweet_id = target_tweets.tweet_id
            LEFT JOIN effective_counts
              ON effective_counts.tweet_id = target_tweets.tweet_id
            "#,
        )
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

    pub async fn sync_tweet_mention_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetMentionRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH target_tweets AS (
                SELECT DISTINCT UNNEST($1::BIGINT[]) AS tweet_id
            ),
            existing_tweets AS (
                SELECT target_tweets.tweet_id
                FROM target_tweets
                JOIN tweet.tweet AS t
                  ON t.id = target_tweets.tweet_id
            ),
            input_refs AS (
                SELECT DISTINCT ON (item.tweet_id, item.user_id)
                    item.tweet_id,
                    item.user_id
                FROM jsonb_to_recordset($2::jsonb) AS item(
                    tweet_id BIGINT,
                    user_id BIGINT
                )
                JOIN existing_tweets
                  ON existing_tweets.tweet_id = item.tweet_id
                ORDER BY item.tweet_id, item.user_id
            ),
            desired_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(input_refs.user_id ORDER BY input_refs.user_id)
                            FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN input_refs
                  ON input_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            current_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(current_refs.user_id ORDER BY current_refs.user_id)
                            FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN tweet.tweet_mention_ref AS current_refs
                  ON current_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            changed_tweets AS (
                SELECT desired_state.tweet_id
                FROM desired_state
                JOIN current_state
                  ON current_state.tweet_id = desired_state.tweet_id
                WHERE desired_state.refs IS DISTINCT FROM current_state.refs
            ),
            deleted AS (
                DELETE FROM tweet.tweet_mention_ref
                WHERE tweet_id IN (SELECT tweet_id FROM changed_tweets)
            ),
            inserted AS (
                INSERT INTO tweet.tweet_mention_ref (tweet_id, user_id)
                SELECT input_refs.tweet_id, input_refs.user_id
                FROM input_refs
                JOIN changed_tweets
                  ON changed_tweets.tweet_id = input_refs.tweet_id
            )
            SELECT
                target_tweets.tweet_id,
                CASE
                    WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
                    WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
                    ELSE 'unchanged'
                END AS status
            FROM target_tweets
            LEFT JOIN existing_tweets
              ON existing_tweets.tweet_id = target_tweets.tweet_id
            LEFT JOIN changed_tweets
              ON changed_tweets.tweet_id = target_tweets.tweet_id
            "#,
        )
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

    pub async fn sync_tweet_hashtag_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetHashtagRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH target_tweets AS (
                SELECT DISTINCT UNNEST($1::BIGINT[]) AS tweet_id
            ),
            existing_tweets AS (
                SELECT target_tweets.tweet_id
                FROM target_tweets
                JOIN tweet.tweet AS t
                  ON t.id = target_tweets.tweet_id
            ),
            input_refs AS (
                SELECT DISTINCT ON (item.tweet_id, item.hashtag_id)
                    item.tweet_id,
                    item.hashtag_id
                FROM jsonb_to_recordset($2::jsonb) AS item(
                    tweet_id BIGINT,
                    hashtag_id INTEGER
                )
                JOIN existing_tweets
                  ON existing_tweets.tweet_id = item.tweet_id
                ORDER BY item.tweet_id, item.hashtag_id
            ),
            desired_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(input_refs.hashtag_id ORDER BY input_refs.hashtag_id)
                            FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN input_refs
                  ON input_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            current_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(current_refs.hashtag_id ORDER BY current_refs.hashtag_id)
                            FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN tweet.tweet_hashtag_ref AS current_refs
                  ON current_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            changed_tweets AS (
                SELECT desired_state.tweet_id
                FROM desired_state
                JOIN current_state
                  ON current_state.tweet_id = desired_state.tweet_id
                WHERE desired_state.refs IS DISTINCT FROM current_state.refs
            ),
            deleted AS (
                DELETE FROM tweet.tweet_hashtag_ref
                WHERE tweet_id IN (SELECT tweet_id FROM changed_tweets)
            ),
            inserted AS (
                INSERT INTO tweet.tweet_hashtag_ref (tweet_id, hashtag_id)
                SELECT input_refs.tweet_id, input_refs.hashtag_id
                FROM input_refs
                JOIN changed_tweets
                  ON changed_tweets.tweet_id = input_refs.tweet_id
            )
            SELECT
                target_tweets.tweet_id,
                CASE
                    WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
                    WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
                    ELSE 'unchanged'
                END AS status
            FROM target_tweets
            LEFT JOIN existing_tweets
              ON existing_tweets.tweet_id = target_tweets.tweet_id
            LEFT JOIN changed_tweets
              ON changed_tweets.tweet_id = target_tweets.tweet_id
            "#,
        )
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

    pub async fn sync_tweet_symbol_refs(
        &self,
        tweet_ids: &[i64],
        refs: &[TweetSymbolRef],
    ) -> AppResult<HashMap<i64, RelationSyncStatus>> {
        if tweet_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            WITH target_tweets AS (
                SELECT DISTINCT UNNEST($1::BIGINT[]) AS tweet_id
            ),
            existing_tweets AS (
                SELECT target_tweets.tweet_id
                FROM target_tweets
                JOIN tweet.tweet AS t
                  ON t.id = target_tweets.tweet_id
            ),
            input_refs AS (
                SELECT DISTINCT ON (item.tweet_id, item.symbol_id)
                    item.tweet_id,
                    item.symbol_id
                FROM jsonb_to_recordset($2::jsonb) AS item(
                    tweet_id BIGINT,
                    symbol_id INTEGER
                )
                JOIN existing_tweets
                  ON existing_tweets.tweet_id = item.tweet_id
                ORDER BY item.tweet_id, item.symbol_id
            ),
            desired_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(input_refs.symbol_id ORDER BY input_refs.symbol_id)
                            FILTER (WHERE input_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN input_refs
                  ON input_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            current_state AS (
                SELECT
                    existing_tweets.tweet_id,
                    COALESCE(
                        jsonb_agg(current_refs.symbol_id ORDER BY current_refs.symbol_id)
                            FILTER (WHERE current_refs.tweet_id IS NOT NULL),
                        '[]'::jsonb
                    ) AS refs
                FROM existing_tweets
                LEFT JOIN tweet.tweet_symbol_ref AS current_refs
                  ON current_refs.tweet_id = existing_tweets.tweet_id
                GROUP BY existing_tweets.tweet_id
            ),
            changed_tweets AS (
                SELECT desired_state.tweet_id
                FROM desired_state
                JOIN current_state
                  ON current_state.tweet_id = desired_state.tweet_id
                WHERE desired_state.refs IS DISTINCT FROM current_state.refs
            ),
            deleted AS (
                DELETE FROM tweet.tweet_symbol_ref
                WHERE tweet_id IN (SELECT tweet_id FROM changed_tweets)
            ),
            inserted AS (
                INSERT INTO tweet.tweet_symbol_ref (tweet_id, symbol_id)
                SELECT input_refs.tweet_id, input_refs.symbol_id
                FROM input_refs
                JOIN changed_tweets
                  ON changed_tweets.tweet_id = input_refs.tweet_id
            )
            SELECT
                target_tweets.tweet_id,
                CASE
                    WHEN existing_tweets.tweet_id IS NULL THEN 'missing_tweet'
                    WHEN changed_tweets.tweet_id IS NOT NULL THEN 'replaced'
                    ELSE 'unchanged'
                END AS status
            FROM target_tweets
            LEFT JOIN existing_tweets
              ON existing_tweets.tweet_id = target_tweets.tweet_id
            LEFT JOIN changed_tweets
              ON changed_tweets.tweet_id = target_tweets.tweet_id
            "#,
        )
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

    pub async fn preload_submit_batch_dicts(
        &self,
        snapshots: &[UserSnapshot],
        places: &[TweetPlace],
        tweets: &[Tweet],
        policies: &[TweetPolicy],
        notes: &[TweetCommunityNote],
        media: &[Media],
        resources: &[MediaResource],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_user_snapshot_dicts(&mut entries, snapshots);
        collect_tweet_place_dicts(&mut entries, places);
        collect_tweet_dicts(&mut entries, tweets);
        collect_tweet_policy_dicts(&mut entries, policies);
        collect_tweet_community_note_dicts(&mut entries, notes);
        collect_media_dicts(&mut entries, media);
        collect_media_resource_dicts(&mut entries, resources);
        self.preload_dict_entries(entries).await
    }

    async fn preload_dict_entries(&self, entries: Vec<(StringSemantic, String)>) -> AppResult<()> {
        self.string_dict.ensure_many(self.pool, entries).await
    }

    async fn preload_user_snapshot_dicts(&self, snapshots: &[UserSnapshot]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_user_snapshot_dicts(&mut entries, snapshots);
        self.preload_dict_entries(entries).await
    }

    async fn preload_tweet_place_dicts(&self, places: &[TweetPlace]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_place_dicts(&mut entries, places);
        self.preload_dict_entries(entries).await
    }

    async fn preload_tweet_dicts(&self, tweets: &[Tweet]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_dicts(&mut entries, tweets);
        self.preload_dict_entries(entries).await
    }

    async fn preload_tweet_policy_dicts(&self, policies: &[TweetPolicy]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_policy_dicts(&mut entries, policies);
        self.preload_dict_entries(entries).await
    }

    async fn preload_tweet_community_note_dicts(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_community_note_dicts(&mut entries, notes);
        self.preload_dict_entries(entries).await
    }

    async fn preload_media_dicts(&self, media: &[Media]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_media_dicts(&mut entries, media);
        self.preload_dict_entries(entries).await
    }

    async fn preload_media_resource_dicts(&self, resources: &[MediaResource]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_media_resource_dicts(&mut entries, resources);
        self.preload_dict_entries(entries).await
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

fn push_optional_entry(
    entries: &mut Vec<(StringSemantic, String)>,
    semantic: StringSemantic,
    value: Option<&str>,
) {
    if let Some(value) = value {
        entries.push((semantic, value.to_owned()));
    }
}

fn collect_user_snapshot_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    snapshots: &[UserSnapshot],
) {
    for snapshot in snapshots {
        push_optional_entry(
            entries,
            StringSemantic::TweetUserAvatarShape,
            snapshot.avatar_shape.as_deref(),
        );
        if let Some(bio) = snapshot.bio.as_ref() {
            collect_annotated_text_dicts(entries, bio);
        }
        if let Some(identity) = snapshot.identity.as_ref() {
            if let Some(verification) = identity.verification.as_ref() {
                push_optional_entry(
                    entries,
                    StringSemantic::TweetUserVerificationType,
                    verification.verified_type.as_deref(),
                );
            }
            if let Some(disclosure) = identity.disclosure.as_ref() {
                push_optional_entry(
                    entries,
                    StringSemantic::TweetUserDisclosureRelation,
                    disclosure.relation.as_deref(),
                );
            }
            push_optional_entry(
                entries,
                StringSemantic::TweetUserParodyLabel,
                identity.parody_label.as_deref(),
            );
        }
        if let Some(professional) = snapshot.professional.as_ref() {
            push_optional_entry(
                entries,
                StringSemantic::TweetUserProfessionalType,
                professional.professional_type.as_deref(),
            );
        }
    }
}

fn collect_tweet_place_dicts(entries: &mut Vec<(StringSemantic, String)>, places: &[TweetPlace]) {
    for place in places {
        push_optional_entry(
            entries,
            StringSemantic::TweetCountryName,
            place.country.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetCountryCode,
            place.country_code.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetPlaceKind,
            place.kind.as_deref(),
        );
    }
}

fn collect_tweet_dicts(entries: &mut Vec<(StringSemantic, String)>, tweets: &[Tweet]) {
    for tweet in tweets {
        push_optional_entry(
            entries,
            StringSemantic::TweetSource,
            tweet.source.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetLanguageCode,
            tweet.language.as_deref(),
        );
        collect_annotated_text_dicts(entries, &tweet.legacy_text);
        if let Some(note_text) = tweet.note_text.as_ref() {
            collect_annotated_text_dicts(entries, note_text);
        }
    }
}

fn collect_tweet_policy_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    policies: &[TweetPolicy],
) {
    for policy in policies {
        push_optional_entry(
            entries,
            StringSemantic::TweetReplyPolicyCode,
            policy.reply_policy.as_deref(),
        );
        for action in &policy.available_actions {
            entries.push((StringSemantic::TweetActionCode, action.clone()));
        }
    }
}

fn collect_tweet_community_note_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    notes: &[TweetCommunityNote],
) {
    for note in notes {
        if let Some(subtitle) = note.subtitle.as_ref() {
            collect_annotated_text_dicts(entries, subtitle);
        }
        if let Some(footer) = note.footer.as_ref() {
            collect_annotated_text_dicts(entries, footer);
        }
    }
}

fn collect_media_dicts(entries: &mut Vec<(StringSemantic, String)>, media: &[Media]) {
    for item in media {
        if let Some(size_variants) = item.size_variants.as_ref() {
            collect_optional_media_size_variant_dicts(entries, size_variants.large.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.medium.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.small.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.thumb.as_ref());
        }
        for tag in &item.tagged_users {
            push_optional_entry(
                entries,
                StringSemantic::TweetMediaTagKind,
                tag.kind.as_deref(),
            );
        }
        for warning in &item.sensitivity_warnings {
            entries.push((StringSemantic::TweetMediaSensitivityCode, warning.clone()));
        }
    }
}

fn collect_media_resource_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    resources: &[MediaResource],
) {
    for resource in resources {
        push_optional_entry(
            entries,
            StringSemantic::TweetMediaAvailabilityStatus,
            resource.availability.as_deref(),
        );
        if let Some(video) = resource.video.as_ref() {
            collect_media_video_dicts(entries, video);
        }
    }
}

fn collect_annotated_text_dicts(entries: &mut Vec<(StringSemantic, String)>, text: &AnnotatedText) {
    for style in &text.styles {
        for name in &style.styles {
            entries.push((StringSemantic::TweetTextStyleName, name.clone()));
        }
    }
}

fn collect_optional_media_size_variant_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    variant: Option<&MediaSizeVariant>,
) {
    if let Some(variant) = variant {
        entries.push((
            StringSemantic::TweetMediaResizeMode,
            variant.resize_mode.clone(),
        ));
    }
}

fn collect_media_video_dicts(entries: &mut Vec<(StringSemantic, String)>, video: &MediaVideo) {
    for variant in &video.variants {
        entries.push((
            StringSemantic::TweetVideoContentType,
            variant.content_type.clone(),
        ));
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UserStatsRow {
    recorded_at: time::OffsetDateTime,
    followers: Option<i64>,
    following: Option<i64>,
    likes: Option<i64>,
    media_posts: Option<i64>,
    tweets: Option<i64>,
    listed: Option<i64>,
}

impl UserStatsRow {
    fn same_user_stats(&self, value: &UserStats) -> bool {
        self.followers == value.followers
            && self.following == value.following
            && self.likes == value.likes
            && self.media_posts == value.media_posts
            && self.tweets == value.tweets
            && self.listed == value.listed
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TweetStatsRow {
    recorded_at: time::OffsetDateTime,
    views: Option<i64>,
    replies: Option<i64>,
    reposts: Option<i64>,
    quotes: Option<i64>,
    likes: Option<i64>,
    bookmarks: Option<i64>,
}

impl TweetStatsRow {
    fn same_tweet_stats(&self, value: &TweetStats) -> bool {
        self.views == value.views
            && self.replies == value.replies
            && self.reposts == value.reposts
            && self.quotes == value.quotes
            && self.likes == value.likes
            && self.bookmarks == value.bookmarks
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JsonRowI64 {
    id: i64,
    data: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct JsonRowString {
    id: String,
    data: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct TweetMediaRefRow {
    tweet_id: i64,
    media_id: i64,
    display_order: i16,
}

#[derive(Debug, sqlx::FromRow)]
struct UserCategoryRow {
    id: i16,
    source_category_code: i32,
    name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct HashtagRow {
    id: i32,
    tag: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SymbolRow {
    id: i32,
    symbol: String,
    ticker: Option<String>,
    name: Option<String>,
}

fn conditional_write_from_db(value: &str) -> AppResult<ConditionalWrite> {
    match value {
        "inserted" => Ok(ConditionalWrite::Inserted),
        "duplicate" => Ok(ConditionalWrite::SkippedDuplicate),
        "unchanged" => Ok(ConditionalWrite::SkippedUnchanged),
        "interval" => Ok(ConditionalWrite::SkippedInterval),
        "missing_parent" => Ok(ConditionalWrite::SkippedMissingParent),
        other => Err(AppError::upstream(format!(
            "unexpected conditional write status: {other}"
        ))),
    }
}

fn relation_sync_status_from_db(value: &str) -> AppResult<RelationSyncStatus> {
    match value {
        "replaced" => Ok(RelationSyncStatus::Replaced),
        "replaced_filtered" => Ok(RelationSyncStatus::ReplacedFiltered),
        "unchanged" => Ok(RelationSyncStatus::SkippedUnchanged),
        "unchanged_filtered" => Ok(RelationSyncStatus::SkippedUnchangedFiltered),
        "missing_tweet" => Ok(RelationSyncStatus::SkippedMissingTweet),
        other => Err(AppError::upstream(format!(
            "unexpected relation sync status: {other}"
        ))),
    }
}
