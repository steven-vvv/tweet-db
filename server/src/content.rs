use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SubmissionEnvelope {
    pub source_kind: String,
    pub client_context: Value,
    pub captures: Vec<CapturedXhrInput>,
    pub users: Vec<XUserInput>,
    pub tweets: Vec<XTweetInput>,
    pub media: Vec<XMediaInput>,
    pub timeline_events: Vec<TimelineObservationInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CapturedXhrInput {
    pub id: String,
    pub timestamp: i64,
    pub method: String,
    pub url: String,
    pub graphql_id: String,
    pub operation_name: String,
    pub status: i32,
    pub status_text: String,
    pub response_headers: String,
    pub response_body: String,
    pub response_size: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct XUserInput {
    pub id: String,
    pub name: String,
    pub screen_name: String,
    pub description: String,
    pub location: String,
    pub avatar_url: String,
    pub profile_url: Option<String>,
    pub banner_url: Option<String>,
    pub is_blue_verified: bool,
    pub verified_type: Option<String>,
    pub is_protected: bool,
    pub profile_image_shape: String,
    pub professional_type: Option<String>,
    pub followers_count: i64,
    pub friends_count: i64,
    pub favourites_count: i64,
    pub statuses_count: i64,
    pub media_count: i64,
    pub listed_count: i64,
    pub pinned_tweet_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct XTweetInput {
    pub id: String,
    pub author_id: String,
    pub conversation_id: String,
    pub full_text: String,
    pub legacy_full_text: String,
    pub note_text: Option<String>,
    pub lang: String,
    pub created_at: String,
    pub in_reply_to_tweet_id: Option<String>,
    pub in_reply_to_user_id: Option<String>,
    pub quoted_tweet_id: Option<String>,
    pub retweeted_tweet_id: Option<String>,
    pub view_count: Option<i64>,
    pub possibly_sensitive: Option<bool>,
    pub favorite_count: i64,
    pub retweet_count: i64,
    pub reply_count: i64,
    pub quote_count: i64,
    pub bookmark_count: i64,
    pub media_ids: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoVariantInput {
    pub bitrate: Option<i64>,
    pub content_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct XMediaInput {
    pub id: String,
    pub media_key: String,
    pub tweet_id: String,
    pub r#type: String,
    pub media_url: String,
    pub thumb_url: String,
    pub source_url: String,
    pub width: i32,
    pub height: i32,
    pub alt_text: Option<String>,
    pub allow_download: bool,
    pub source_status_id: Option<String>,
    pub source_user_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub video_variants: Vec<VideoVariantInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TimelineObservationInput {
    pub timeline_kind: String,
    pub timeline_key: String,
    pub post_ids: Vec<String>,
    pub observed_at_ms: Option<i64>,
    pub raw: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub submission_id: Uuid,
    pub status: String,
    pub accepted_count: i32,
    pub warnings: Vec<String>,
}

pub async fn ingest_submission(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<SubmissionEnvelope>,
) -> AppResult<Json<IngestResponse>> {
    let session = auth::require_registered_session(session)?;
    let source_kind = validate_source_kind(&payload.source_kind)?;
    validate_batch_size(&state, &payload)?;

    let submission_id = Uuid::now_v7();
    let now = OffsetDateTime::now_utc();
    let request_body = serde_json::to_value(&payload)?;
    insert_submission(
        &state.db,
        submission_id,
        session.record.user_id,
        &source_kind,
        &payload,
        request_body,
    )
    .await?;

    let mut accepted_count = 0_i32;
    let mut warnings = Vec::new();

    for user in &payload.users {
        match upsert_actor(&state.db, submission_id, &source_kind, user, now).await {
            Ok(true) => accepted_count += 1,
            Ok(false) => warnings.push("skipped user with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert user {}: {error}", user.id)),
        }
    }

    for tweet in &payload.tweets {
        match upsert_post(&state.db, submission_id, &source_kind, tweet, now).await {
            Ok(true) => {
                accepted_count += 1;
                insert_post_metric_observation(&state.db, submission_id, &source_kind, tweet, now)
                    .await?;
            }
            Ok(false) => warnings.push("skipped tweet with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert tweet {}: {error}", tweet.id)),
        }
    }

    for media in &payload.media {
        match upsert_media(&state.db, submission_id, &source_kind, media, now).await {
            Ok(true) => {
                accepted_count += 1;
                insert_media_variants(&state.db, submission_id, &source_kind, media, now).await?;
            }
            Ok(false) => warnings.push("skipped media with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert media {}: {error}", media.id)),
        }
    }

    for capture in &payload.captures {
        match insert_capture(&state.db, submission_id, &source_kind, capture).await {
            Ok(true) => accepted_count += 1,
            Ok(false) => warnings.push("skipped capture with empty id".to_owned()),
            Err(error) => {
                warnings.push(format!("failed to insert capture {}: {error}", capture.id))
            }
        }
    }

    for timeline in &payload.timeline_events {
        match insert_timeline_observation(&state.db, submission_id, &source_kind, timeline, now)
            .await
        {
            Ok(true) => accepted_count += 1,
            Ok(false) => warnings.push("skipped timeline observation with empty key".to_owned()),
            Err(error) => warnings.push(format!(
                "failed to insert timeline observation {}: {error}",
                timeline.timeline_key
            )),
        }
    }

    let status = if warnings.is_empty() {
        "success"
    } else {
        "partial"
    };

    finalize_submission(
        &state.db,
        submission_id,
        status,
        accepted_count,
        &warnings,
        now,
    )
    .await?;

    Ok(Json(IngestResponse {
        submission_id,
        status: status.to_owned(),
        accepted_count,
        warnings,
    }))
}

fn validate_source_kind(raw: &str) -> AppResult<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(AppError::bad_request("sourceKind is required"));
    }
    Ok(value)
}

fn validate_batch_size(state: &AppState, payload: &SubmissionEnvelope) -> AppResult<()> {
    let total = payload.users.len()
        + payload.tweets.len()
        + payload.media.len()
        + payload.captures.len()
        + payload.timeline_events.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "batch exceeds max_items_per_batch ({})",
            state.settings.config.ingest.max_items_per_batch
        )));
    }
    Ok(())
}

async fn insert_submission(
    pool: &PgPool,
    submission_id: Uuid,
    submitter_user_id: Option<Uuid>,
    source_kind: &str,
    payload: &SubmissionEnvelope,
    request_body: Value,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO ingest_submissions (
            id,
            submitter_user_id,
            source_kind,
            client_context,
            request_body,
            users_count,
            tweets_count,
            media_count,
            captures_count,
            timeline_events_count,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'processing')
        "#,
    )
    .bind(submission_id)
    .bind(submitter_user_id)
    .bind(source_kind)
    .bind(&payload.client_context)
    .bind(request_body)
    .bind(payload.users.len() as i32)
    .bind(payload.tweets.len() as i32)
    .bind(payload.media.len() as i32)
    .bind(payload.captures.len() as i32)
    .bind(payload.timeline_events.len() as i32)
    .execute(pool)
    .await?;
    Ok(())
}

async fn finalize_submission(
    pool: &PgPool,
    submission_id: Uuid,
    status: &str,
    accepted_count: i32,
    warnings: &[String],
    processed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE ingest_submissions
        SET status = $2,
            accepted_count = $3,
            warnings = $4,
            processed_at = $5
        WHERE id = $1
        "#,
    )
    .bind(submission_id)
    .bind(status)
    .bind(accepted_count)
    .bind(serde_json::to_value(warnings)?)
    .bind(processed_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_actor(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    sqlx::query(
        r#"
        INSERT INTO actors (
            id,
            source_kind,
            source_actor_id,
            name,
            screen_name,
            description,
            location,
            avatar_url,
            profile_url,
            banner_url,
            is_blue_verified,
            verified_type,
            is_protected,
            profile_image_shape,
            professional_type,
            followers_count,
            friends_count,
            favourites_count,
            statuses_count,
            media_count,
            listed_count,
            pinned_post_source_ids,
            source_created_at_raw,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at,
            raw_json
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23, $24, $24, $25, $25, $26
        )
        ON CONFLICT (source_kind, source_actor_id) DO UPDATE
        SET name = EXCLUDED.name,
            screen_name = EXCLUDED.screen_name,
            description = EXCLUDED.description,
            location = EXCLUDED.location,
            avatar_url = EXCLUDED.avatar_url,
            profile_url = EXCLUDED.profile_url,
            banner_url = EXCLUDED.banner_url,
            is_blue_verified = EXCLUDED.is_blue_verified,
            verified_type = EXCLUDED.verified_type,
            is_protected = EXCLUDED.is_protected,
            profile_image_shape = EXCLUDED.profile_image_shape,
            professional_type = EXCLUDED.professional_type,
            followers_count = EXCLUDED.followers_count,
            friends_count = EXCLUDED.friends_count,
            favourites_count = EXCLUDED.favourites_count,
            statuses_count = EXCLUDED.statuses_count,
            media_count = EXCLUDED.media_count,
            listed_count = EXCLUDED.listed_count,
            pinned_post_source_ids = EXCLUDED.pinned_post_source_ids,
            source_created_at_raw = EXCLUDED.source_created_at_raw,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            raw_json = EXCLUDED.raw_json,
            updated_at = NOW()
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(source_kind)
    .bind(&item.id)
    .bind(&item.name)
    .bind(&item.screen_name)
    .bind(&item.description)
    .bind(&item.location)
    .bind(&item.avatar_url)
    .bind(&item.profile_url)
    .bind(&item.banner_url)
    .bind(item.is_blue_verified)
    .bind(&item.verified_type)
    .bind(item.is_protected)
    .bind(&item.profile_image_shape)
    .bind(&item.professional_type)
    .bind(item.followers_count)
    .bind(item.friends_count)
    .bind(item.favourites_count)
    .bind(item.statuses_count)
    .bind(item.media_count)
    .bind(item.listed_count)
    .bind(&item.pinned_tweet_ids)
    .bind(&item.created_at)
    .bind(submission_id)
    .bind(observed_at)
    .bind(serde_json::to_value(item)?)
    .execute(pool)
    .await?;

    Ok(true)
}

async fn upsert_post(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XTweetInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    sqlx::query(
        r#"
        INSERT INTO posts (
            id,
            source_kind,
            source_post_id,
            author_source_actor_id,
            conversation_source_post_id,
            full_text,
            legacy_full_text,
            note_text,
            lang,
            source_created_at_raw,
            in_reply_to_source_post_id,
            in_reply_to_source_actor_id,
            quoted_source_post_id,
            retweeted_source_post_id,
            view_count,
            possibly_sensitive,
            favorite_count,
            retweet_count,
            reply_count,
            quote_count,
            bookmark_count,
            media_source_ids,
            source_label,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at,
            raw_json
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23, $24, $24, $25, $25, $26
        )
        ON CONFLICT (source_kind, source_post_id) DO UPDATE
        SET author_source_actor_id = EXCLUDED.author_source_actor_id,
            conversation_source_post_id = EXCLUDED.conversation_source_post_id,
            full_text = EXCLUDED.full_text,
            legacy_full_text = EXCLUDED.legacy_full_text,
            note_text = EXCLUDED.note_text,
            lang = EXCLUDED.lang,
            source_created_at_raw = EXCLUDED.source_created_at_raw,
            in_reply_to_source_post_id = EXCLUDED.in_reply_to_source_post_id,
            in_reply_to_source_actor_id = EXCLUDED.in_reply_to_source_actor_id,
            quoted_source_post_id = EXCLUDED.quoted_source_post_id,
            retweeted_source_post_id = EXCLUDED.retweeted_source_post_id,
            view_count = EXCLUDED.view_count,
            possibly_sensitive = EXCLUDED.possibly_sensitive,
            favorite_count = EXCLUDED.favorite_count,
            retweet_count = EXCLUDED.retweet_count,
            reply_count = EXCLUDED.reply_count,
            quote_count = EXCLUDED.quote_count,
            bookmark_count = EXCLUDED.bookmark_count,
            media_source_ids = EXCLUDED.media_source_ids,
            source_label = EXCLUDED.source_label,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            raw_json = EXCLUDED.raw_json,
            updated_at = NOW()
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(source_kind)
    .bind(&item.id)
    .bind(&item.author_id)
    .bind(&item.conversation_id)
    .bind(&item.full_text)
    .bind(&item.legacy_full_text)
    .bind(&item.note_text)
    .bind(&item.lang)
    .bind(&item.created_at)
    .bind(&item.in_reply_to_tweet_id)
    .bind(&item.in_reply_to_user_id)
    .bind(&item.quoted_tweet_id)
    .bind(&item.retweeted_tweet_id)
    .bind(item.view_count)
    .bind(item.possibly_sensitive)
    .bind(item.favorite_count)
    .bind(item.retweet_count)
    .bind(item.reply_count)
    .bind(item.quote_count)
    .bind(item.bookmark_count)
    .bind(&item.media_ids)
    .bind(&item.source)
    .bind(submission_id)
    .bind(observed_at)
    .bind(serde_json::to_value(item)?)
    .execute(pool)
    .await?;

    Ok(true)
}

async fn upsert_media(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    sqlx::query(
        r#"
        INSERT INTO media (
            id,
            source_kind,
            source_media_id,
            media_key,
            source_post_id,
            media_type,
            media_url,
            thumb_url,
            source_url,
            width,
            height,
            alt_text,
            allow_download,
            source_status_id,
            source_actor_id,
            duration_ms,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at,
            raw_json
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $17, $18, $18, $19
        )
        ON CONFLICT (source_kind, source_media_id) DO UPDATE
        SET media_key = EXCLUDED.media_key,
            source_post_id = EXCLUDED.source_post_id,
            media_type = EXCLUDED.media_type,
            media_url = EXCLUDED.media_url,
            thumb_url = EXCLUDED.thumb_url,
            source_url = EXCLUDED.source_url,
            width = EXCLUDED.width,
            height = EXCLUDED.height,
            alt_text = EXCLUDED.alt_text,
            allow_download = EXCLUDED.allow_download,
            source_status_id = EXCLUDED.source_status_id,
            source_actor_id = EXCLUDED.source_actor_id,
            duration_ms = EXCLUDED.duration_ms,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            raw_json = EXCLUDED.raw_json,
            updated_at = NOW()
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(source_kind)
    .bind(&item.id)
    .bind(&item.media_key)
    .bind(&item.tweet_id)
    .bind(&item.r#type)
    .bind(&item.media_url)
    .bind(&item.thumb_url)
    .bind(&item.source_url)
    .bind(item.width)
    .bind(item.height)
    .bind(&item.alt_text)
    .bind(item.allow_download)
    .bind(&item.source_status_id)
    .bind(&item.source_user_id)
    .bind(item.duration_ms)
    .bind(submission_id)
    .bind(observed_at)
    .bind(serde_json::to_value(item)?)
    .execute(pool)
    .await?;

    Ok(true)
}

async fn insert_capture(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &CapturedXhrInput,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    sqlx::query(
        r#"
        INSERT INTO capture_events (
            id,
            submission_id,
            source_kind,
            capture_id,
            captured_at,
            method,
            url,
            graphql_id,
            operation_name,
            status,
            status_text,
            response_headers,
            response_body,
            response_size,
            raw_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(submission_id)
    .bind(source_kind)
    .bind(&item.id)
    .bind(ms_to_time(item.timestamp).unwrap_or_else(OffsetDateTime::now_utc))
    .bind(&item.method)
    .bind(&item.url)
    .bind(&item.graphql_id)
    .bind(&item.operation_name)
    .bind(item.status)
    .bind(&item.status_text)
    .bind(&item.response_headers)
    .bind(&item.response_body)
    .bind(item.response_size)
    .bind(serde_json::to_value(item)?)
    .execute(pool)
    .await?;

    Ok(true)
}

async fn insert_post_metric_observation(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XTweetInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO post_metric_observations (
            id,
            submission_id,
            source_kind,
            source_post_id,
            observed_at,
            view_count,
            favorite_count,
            retweet_count,
            reply_count,
            quote_count,
            bookmark_count,
            raw_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(submission_id)
    .bind(source_kind)
    .bind(&item.id)
    .bind(observed_at)
    .bind(item.view_count)
    .bind(item.favorite_count)
    .bind(item.retweet_count)
    .bind(item.reply_count)
    .bind(item.quote_count)
    .bind(item.bookmark_count)
    .bind(serde_json::to_value(item)?)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_media_variants(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    for variant in &item.video_variants {
        if variant.url.trim().is_empty() {
            continue;
        }

        sqlx::query(
            r#"
            INSERT INTO media_variant_observations (
                id,
                submission_id,
                source_kind,
                source_media_id,
                bitrate,
                content_type,
                url,
                observed_at,
                raw_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(submission_id)
        .bind(source_kind)
        .bind(&item.id)
        .bind(variant.bitrate)
        .bind(&variant.content_type)
        .bind(&variant.url)
        .bind(observed_at)
        .bind(serde_json::to_value(variant)?)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn insert_timeline_observation(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &TimelineObservationInput,
    fallback_observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.timeline_kind.trim().is_empty() || item.timeline_key.trim().is_empty() {
        return Ok(false);
    }

    let raw_json = if item.raw.is_null() {
        serde_json::json!({
            "timelineKind": item.timeline_kind,
            "timelineKey": item.timeline_key,
            "postIds": item.post_ids,
            "observedAtMs": item.observed_at_ms,
        })
    } else {
        item.raw.clone()
    };

    sqlx::query(
        r#"
        INSERT INTO timeline_observations (
            id,
            submission_id,
            source_kind,
            timeline_kind,
            timeline_key,
            post_source_ids,
            observed_at,
            raw_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(submission_id)
    .bind(source_kind)
    .bind(&item.timeline_kind)
    .bind(&item.timeline_key)
    .bind(&item.post_ids)
    .bind(
        item.observed_at_ms
            .and_then(ms_to_time)
            .unwrap_or(fallback_observed_at),
    )
    .bind(raw_json)
    .execute(pool)
    .await?;

    Ok(true)
}

fn ms_to_time(value: i64) -> Option<OffsetDateTime> {
    let seconds = value.div_euclid(1000);
    let millis = value.rem_euclid(1000);
    OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .map(|time| time + Duration::milliseconds(millis))
}
