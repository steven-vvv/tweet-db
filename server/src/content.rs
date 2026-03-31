use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostStatusQueryRequest {
    pub source_kind: String,
    pub post_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostStatusQueryResponse {
    pub items: Vec<PostStatusAggregate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostStatusAggregate {
    pub source_kind: String,
    pub post_id: String,
    pub found: bool,
    pub post: Option<PostView>,
    pub author: Option<ActorView>,
    pub media: Vec<MediaStatusView>,
    pub missing_media_source_ids: Vec<String>,
    pub timeline_hits: Vec<TimelineHit>,
    pub capture_summary: Option<CaptureSummary>,
    pub transfer_summary: TransferSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostView {
    pub source_post_id: String,
    pub author_source_actor_id: String,
    pub conversation_source_post_id: String,
    pub full_text: String,
    pub legacy_full_text: String,
    pub note_text: Option<String>,
    pub lang: String,
    pub source_created_at_raw: String,
    pub in_reply_to_source_post_id: Option<String>,
    pub in_reply_to_source_actor_id: Option<String>,
    pub quoted_source_post_id: Option<String>,
    pub retweeted_source_post_id: Option<String>,
    pub view_count: Option<i64>,
    pub possibly_sensitive: Option<bool>,
    pub favorite_count: i64,
    pub retweet_count: i64,
    pub reply_count: i64,
    pub quote_count: i64,
    pub bookmark_count: i64,
    pub media_source_ids: Vec<String>,
    pub source_label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorView {
    pub source_actor_id: String,
    pub name: String,
    pub screen_name: String,
    pub description: String,
    pub location: String,
    pub avatar_url: String,
    pub profile_url: Option<String>,
    pub banner_url: Option<String>,
    pub verified_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatusView {
    pub source_media_id: String,
    pub media_key: String,
    pub source_post_id: String,
    pub media_type: String,
    pub source_url: String,
    pub thumb_url: String,
    pub width: i32,
    pub height: i32,
    pub alt_text: Option<String>,
    pub allow_download: bool,
    pub duration_ms: Option<i64>,
    pub transfer_status: Option<String>,
    pub storage_object_key: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineHit {
    pub timeline_kind: String,
    pub timeline_key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSummary {
    pub first_submission_id: Option<Uuid>,
    pub last_submission_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    pub first_observed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_observed_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub pending: u32,
    pub processing: u32,
    pub succeeded: u32,
    pub failed: u32,
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

pub async fn query_post_status(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<PostStatusQueryRequest>,
) -> AppResult<Json<PostStatusQueryResponse>> {
    let _session = auth::require_registered_session(session)?;
    let source_kind = validate_source_kind(&payload.source_kind)?;
    if payload.post_ids.is_empty() {
        return Err(AppError::bad_request("postIds must not be empty"));
    }

    let unique_post_ids = payload
        .post_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if unique_post_ids.is_empty() {
        return Err(AppError::bad_request(
            "postIds must contain at least one non-empty value",
        ));
    }

    let posts = fetch_posts(&state.db, &source_kind, &unique_post_ids).await?;
    let posts_by_id = posts
        .iter()
        .map(|post| (post.source_post_id.clone(), post.clone()))
        .collect::<HashMap<_, _>>();

    let author_ids = posts
        .iter()
        .map(|post| post.author_source_actor_id.clone())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let actors = fetch_actors(&state.db, &source_kind, &author_ids).await?;
    let actors_by_id = actors
        .into_iter()
        .map(|actor| (actor.source_actor_id.clone(), actor))
        .collect::<HashMap<_, _>>();

    let media_ids = posts
        .iter()
        .flat_map(|post| post.media_source_ids.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let media = fetch_media(&state.db, &source_kind, &media_ids).await?;
    let media_by_id = media
        .into_iter()
        .map(|item| (item.source_media_id.clone(), item))
        .collect::<HashMap<_, _>>();

    let timeline_hits = fetch_timeline_hits(&state.db, &source_kind, &unique_post_ids).await?;
    let mut timeline_hits_by_post: HashMap<String, Vec<TimelineHit>> = HashMap::new();
    for (post_id, hit) in timeline_hits {
        timeline_hits_by_post.entry(post_id).or_default().push(hit);
    }

    let items = payload
        .post_ids
        .into_iter()
        .map(|requested_id| {
            let lookup_id = requested_id.trim().to_owned();
            build_post_status_aggregate(
                &source_kind,
                requested_id,
                posts_by_id.get(&lookup_id),
                &actors_by_id,
                &media_by_id,
                timeline_hits_by_post.remove(&lookup_id).unwrap_or_default(),
            )
        })
        .collect();

    Ok(Json(PostStatusQueryResponse { items }))
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

#[derive(Debug, Clone)]
struct PostRow {
    source_post_id: String,
    author_source_actor_id: String,
    conversation_source_post_id: String,
    full_text: String,
    legacy_full_text: String,
    note_text: Option<String>,
    lang: String,
    source_created_at_raw: String,
    in_reply_to_source_post_id: Option<String>,
    in_reply_to_source_actor_id: Option<String>,
    quoted_source_post_id: Option<String>,
    retweeted_source_post_id: Option<String>,
    view_count: Option<i64>,
    possibly_sensitive: Option<bool>,
    favorite_count: i64,
    retweet_count: i64,
    reply_count: i64,
    quote_count: i64,
    bookmark_count: i64,
    media_source_ids: Vec<String>,
    source_label: String,
    first_submission_id: Option<Uuid>,
    last_submission_id: Option<Uuid>,
    first_observed_at: OffsetDateTime,
    last_observed_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
struct ActorRow {
    source_actor_id: String,
    name: String,
    screen_name: String,
    description: String,
    location: String,
    avatar_url: String,
    profile_url: Option<String>,
    banner_url: Option<String>,
    verified_type: Option<String>,
}

#[derive(Debug, Clone)]
struct MediaRow {
    source_media_id: String,
    media_key: String,
    source_post_id: String,
    media_type: String,
    source_url: String,
    thumb_url: String,
    width: i32,
    height: i32,
    alt_text: Option<String>,
    allow_download: bool,
    duration_ms: Option<i64>,
}

async fn fetch_posts(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<Vec<PostRow>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
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
            last_observed_at
        FROM posts
        WHERE source_kind = $1
          AND source_post_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PostRow {
            source_post_id: row.get("source_post_id"),
            author_source_actor_id: row.get("author_source_actor_id"),
            conversation_source_post_id: row.get("conversation_source_post_id"),
            full_text: row.get("full_text"),
            legacy_full_text: row.get("legacy_full_text"),
            note_text: row.get("note_text"),
            lang: row.get("lang"),
            source_created_at_raw: row.get("source_created_at_raw"),
            in_reply_to_source_post_id: row.get("in_reply_to_source_post_id"),
            in_reply_to_source_actor_id: row.get("in_reply_to_source_actor_id"),
            quoted_source_post_id: row.get("quoted_source_post_id"),
            retweeted_source_post_id: row.get("retweeted_source_post_id"),
            view_count: row.get("view_count"),
            possibly_sensitive: row.get("possibly_sensitive"),
            favorite_count: row.get("favorite_count"),
            retweet_count: row.get("retweet_count"),
            reply_count: row.get("reply_count"),
            quote_count: row.get("quote_count"),
            bookmark_count: row.get("bookmark_count"),
            media_source_ids: row.get("media_source_ids"),
            source_label: row.get("source_label"),
            first_submission_id: row.get("first_submission_id"),
            last_submission_id: row.get("last_submission_id"),
            first_observed_at: row.get("first_observed_at"),
            last_observed_at: row.get("last_observed_at"),
        })
        .collect())
}

async fn fetch_actors(
    pool: &PgPool,
    source_kind: &str,
    actor_ids: &[String],
) -> AppResult<Vec<ActorRow>> {
    if actor_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            source_actor_id,
            name,
            screen_name,
            description,
            location,
            avatar_url,
            profile_url,
            banner_url,
            verified_type
        FROM actors
        WHERE source_kind = $1
          AND source_actor_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(actor_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ActorRow {
            source_actor_id: row.get("source_actor_id"),
            name: row.get("name"),
            screen_name: row.get("screen_name"),
            description: row.get("description"),
            location: row.get("location"),
            avatar_url: row.get("avatar_url"),
            profile_url: row.get("profile_url"),
            banner_url: row.get("banner_url"),
            verified_type: row.get("verified_type"),
        })
        .collect())
}

async fn fetch_media(
    pool: &PgPool,
    source_kind: &str,
    media_ids: &[String],
) -> AppResult<Vec<MediaRow>> {
    if media_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            source_media_id,
            media_key,
            source_post_id,
            media_type,
            source_url,
            thumb_url,
            width,
            height,
            alt_text,
            allow_download,
            duration_ms
        FROM media
        WHERE source_kind = $1
          AND source_media_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(media_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| MediaRow {
            source_media_id: row.get("source_media_id"),
            media_key: row.get("media_key"),
            source_post_id: row.get("source_post_id"),
            media_type: row.get("media_type"),
            source_url: row.get("source_url"),
            thumb_url: row.get("thumb_url"),
            width: row.get("width"),
            height: row.get("height"),
            alt_text: row.get("alt_text"),
            allow_download: row.get("allow_download"),
            duration_ms: row.get("duration_ms"),
        })
        .collect())
}

async fn fetch_timeline_hits(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<Vec<(String, TimelineHit)>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT timeline_kind, timeline_key, post_source_ids, observed_at
        FROM timeline_observations
        WHERE source_kind = $1
          AND post_source_ids && $2
        ORDER BY observed_at DESC
        LIMIT 500
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    let requested = post_ids.iter().cloned().collect::<HashSet<_>>();
    let mut hits = Vec::new();

    for row in rows {
        let hit = TimelineHit {
            timeline_kind: row.get("timeline_kind"),
            timeline_key: row.get("timeline_key"),
            observed_at: row.get("observed_at"),
        };
        let post_source_ids: Vec<String> = row.get("post_source_ids");
        for post_id in post_source_ids {
            if requested.contains(&post_id) {
                hits.push((post_id, hit.clone()));
            }
        }
    }

    Ok(hits)
}

fn build_post_status_aggregate(
    source_kind: &str,
    requested_id: String,
    post: Option<&PostRow>,
    actors_by_id: &HashMap<String, ActorRow>,
    media_by_id: &HashMap<String, MediaRow>,
    timeline_hits: Vec<TimelineHit>,
) -> PostStatusAggregate {
    let Some(post) = post else {
        return PostStatusAggregate {
            source_kind: source_kind.to_owned(),
            post_id: requested_id,
            found: false,
            post: None,
            author: None,
            media: Vec::new(),
            missing_media_source_ids: Vec::new(),
            timeline_hits,
            capture_summary: None,
            transfer_summary: TransferSummary::default(),
        };
    };

    let author = actors_by_id
        .get(&post.author_source_actor_id)
        .map(|actor| ActorView {
            source_actor_id: actor.source_actor_id.clone(),
            name: actor.name.clone(),
            screen_name: actor.screen_name.clone(),
            description: actor.description.clone(),
            location: actor.location.clone(),
            avatar_url: actor.avatar_url.clone(),
            profile_url: actor.profile_url.clone(),
            banner_url: actor.banner_url.clone(),
            verified_type: actor.verified_type.clone(),
        });

    let mut media = Vec::new();
    let mut missing_media_source_ids = Vec::new();
    for media_id in &post.media_source_ids {
        if let Some(item) = media_by_id.get(media_id) {
            media.push(MediaStatusView {
                source_media_id: item.source_media_id.clone(),
                media_key: item.media_key.clone(),
                source_post_id: item.source_post_id.clone(),
                media_type: item.media_type.clone(),
                source_url: item.source_url.clone(),
                thumb_url: item.thumb_url.clone(),
                width: item.width,
                height: item.height,
                alt_text: item.alt_text.clone(),
                allow_download: item.allow_download,
                duration_ms: item.duration_ms,
                transfer_status: None,
                storage_object_key: None,
                last_error: None,
            });
        } else {
            missing_media_source_ids.push(media_id.clone());
        }
    }

    PostStatusAggregate {
        source_kind: source_kind.to_owned(),
        post_id: requested_id,
        found: true,
        post: Some(PostView {
            source_post_id: post.source_post_id.clone(),
            author_source_actor_id: post.author_source_actor_id.clone(),
            conversation_source_post_id: post.conversation_source_post_id.clone(),
            full_text: post.full_text.clone(),
            legacy_full_text: post.legacy_full_text.clone(),
            note_text: post.note_text.clone(),
            lang: post.lang.clone(),
            source_created_at_raw: post.source_created_at_raw.clone(),
            in_reply_to_source_post_id: post.in_reply_to_source_post_id.clone(),
            in_reply_to_source_actor_id: post.in_reply_to_source_actor_id.clone(),
            quoted_source_post_id: post.quoted_source_post_id.clone(),
            retweeted_source_post_id: post.retweeted_source_post_id.clone(),
            view_count: post.view_count,
            possibly_sensitive: post.possibly_sensitive,
            favorite_count: post.favorite_count,
            retweet_count: post.retweet_count,
            reply_count: post.reply_count,
            quote_count: post.quote_count,
            bookmark_count: post.bookmark_count,
            media_source_ids: post.media_source_ids.clone(),
            source_label: post.source_label.clone(),
        }),
        author,
        media,
        missing_media_source_ids,
        timeline_hits,
        capture_summary: Some(CaptureSummary {
            first_submission_id: post.first_submission_id,
            last_submission_id: post.last_submission_id,
            first_observed_at: post.first_observed_at,
            last_observed_at: post.last_observed_at,
        }),
        transfer_summary: TransferSummary::default(),
    }
}
