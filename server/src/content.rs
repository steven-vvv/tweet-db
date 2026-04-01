use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    state::AppState,
    transfer::{self, TransferStatusInfo},
};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmissionEnvelope {
    pub source_kind: String,
    pub users: Vec<XUserInput>,
    pub tweets: Vec<XTweetInput>,
    pub media: Vec<XMediaInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoVariantInput {
    pub bitrate: Option<i64>,
    pub content_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub submission_id: Uuid,
    pub status: String,
    pub accepted_count: i32,
    pub transfer_jobs_enqueued: i32,
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

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub pending: u32,
    pub processing: u32,
    pub succeeded: u32,
    pub failed: u32,
}

struct NormalizedItems<T> {
    items: Vec<T>,
    skipped_empty: usize,
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
    insert_submission(
        &state.db,
        submission_id,
        session.record.user_id,
        &source_kind,
        &payload,
    )
    .await?;

    let users = normalize_entities(&payload.users, |item| &item.id);
    let tweets = normalize_entities(&payload.tweets, |item| &item.id);
    let media = normalize_entities(&payload.media, |item| &item.id);

    let mut accepted_count = 0_i32;
    let mut transfer_jobs_enqueued = 0_i32;
    let mut warnings = Vec::new();

    extend_skip_warnings(
        &mut warnings,
        users.skipped_empty,
        "skipped user with empty id",
    );
    extend_skip_warnings(
        &mut warnings,
        tweets.skipped_empty,
        "skipped tweet with empty id",
    );
    extend_skip_warnings(
        &mut warnings,
        media.skipped_empty,
        "skipped media with empty id",
    );

    for user in &users.items {
        match process_actor(&state.db, submission_id, &source_kind, user, now).await {
            Ok(true) => accepted_count += 1,
            Ok(false) => warnings.push("skipped user with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert user {}: {error}", user.id)),
        }
    }

    for tweet in &tweets.items {
        match process_post(&state.db, submission_id, &source_kind, tweet, now).await {
            Ok(true) => accepted_count += 1,
            Ok(false) => warnings.push("skipped tweet with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert tweet {}: {error}", tweet.id)),
        }
    }

    for item in &media.items {
        match process_media(&state.db, submission_id, &source_kind, item, now).await {
            Ok(true) => {
                accepted_count += 1;
                if let Some((source_url, content_type)) = transfer_candidate(item) {
                    match transfer::enqueue_media_transfer(
                        &state.db,
                        &source_kind,
                        item.id.trim(),
                        item.tweet_id.trim(),
                        &source_url,
                        &content_type,
                    )
                    .await
                    {
                        Ok(true) => transfer_jobs_enqueued += 1,
                        Ok(false) => {}
                        Err(error) => warnings.push(format!(
                            "failed to enqueue transfer for media {}: {error}",
                            item.id
                        )),
                    }
                }
            }
            Ok(false) => warnings.push("skipped media with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert media {}: {error}", item.id)),
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
        transfer_jobs_enqueued,
        &warnings,
        now,
    )
    .await?;

    Ok(Json(IngestResponse {
        submission_id,
        status: status.to_owned(),
        accepted_count,
        transfer_jobs_enqueued,
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

    let media_ids_by_post = fetch_post_media_ids(&state.db, &source_kind, &unique_post_ids).await?;
    let media_ids = media_ids_by_post
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let media = fetch_media(&state.db, &source_kind, &media_ids).await?;
    let media_by_id = media
        .into_iter()
        .map(|item| (item.source_media_id.clone(), item))
        .collect::<HashMap<_, _>>();
    let transfer_statuses =
        transfer::fetch_transfer_statuses(&state.db, &source_kind, &media_ids).await?;

    let metrics = fetch_latest_post_metrics(&state.db, &source_kind, &unique_post_ids).await?;
    let metrics_by_post = metrics
        .into_iter()
        .map(|item| (item.source_post_id.clone(), item))
        .collect::<HashMap<_, _>>();

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
                media_ids_by_post.get(&lookup_id),
                &media_by_id,
                &metrics_by_post,
                &transfer_statuses,
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
    let total = payload.users.len() + payload.tweets.len() + payload.media.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "batch exceeds max_items_per_batch ({})",
            state.settings.config.ingest.max_items_per_batch
        )));
    }
    Ok(())
}

fn normalize_entities<T, F>(items: &[T], key_fn: F) -> NormalizedItems<T>
where
    T: Clone,
    F: Fn(&T) -> &str,
{
    let mut skipped_empty = 0_usize;
    let mut keys = Vec::new();
    let mut items_by_key = HashMap::new();

    for item in items {
        let key = key_fn(item).trim();
        if key.is_empty() {
            skipped_empty += 1;
            continue;
        }

        let key = key.to_owned();
        if !items_by_key.contains_key(&key) {
            keys.push(key.clone());
        }
        items_by_key.insert(key, item.clone());
    }

    NormalizedItems {
        items: keys
            .into_iter()
            .filter_map(|key| items_by_key.remove(&key))
            .collect(),
        skipped_empty,
    }
}

fn extend_skip_warnings(target: &mut Vec<String>, count: usize, message: &str) {
    for _ in 0..count {
        target.push(message.to_owned());
    }
}

fn unique_nonempty_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for value in values {
        let normalized = value.trim();
        if normalized.is_empty() {
            continue;
        }

        let normalized = normalized.to_owned();
        if seen.insert(normalized.clone()) {
            result.push(normalized);
        }
    }

    result
}

fn unique_video_variants(values: &[VideoVariantInput]) -> Vec<VideoVariantInput> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for value in values {
        let url = value.url.trim();
        if url.is_empty() {
            continue;
        }

        let normalized = url.to_owned();
        if seen.insert(normalized.clone()) {
            let mut cloned = value.clone();
            cloned.url = normalized;
            result.push(cloned);
        }
    }

    result
}

async fn insert_submission(
    pool: &PgPool,
    submission_id: Uuid,
    submitter_user_id: Option<Uuid>,
    source_kind: &str,
    payload: &SubmissionEnvelope,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO ingest_submissions (
            id,
            submitter_user_id,
            source_kind,
            users_count,
            tweets_count,
            media_count,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'processing')
        "#,
    )
    .bind(submission_id)
    .bind(submitter_user_id)
    .bind(source_kind)
    .bind(payload.users.len() as i32)
    .bind(payload.tweets.len() as i32)
    .bind(payload.media.len() as i32)
    .execute(pool)
    .await?;
    Ok(())
}

async fn finalize_submission(
    pool: &PgPool,
    submission_id: Uuid,
    status: &str,
    accepted_count: i32,
    transfer_jobs_enqueued: i32,
    warnings: &[String],
    processed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE ingest_submissions
        SET status = $2,
            accepted_count = $3,
            transfer_jobs_enqueued = $4,
            warnings = $5,
            processed_at = $6
        WHERE id = $1
        "#,
    )
    .bind(submission_id)
    .bind(status)
    .bind(accepted_count)
    .bind(transfer_jobs_enqueued)
    .bind(serde_json::to_value(warnings)?)
    .bind(processed_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn process_actor(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    upsert_actor(pool, submission_id, source_kind, item, observed_at).await?;
    insert_actor_profile_observation(pool, submission_id, source_kind, item, observed_at).await?;
    insert_actor_metric_observation(pool, submission_id, source_kind, item, observed_at).await?;
    Ok(true)
}

async fn process_post(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XTweetInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    upsert_post(pool, submission_id, source_kind, item, observed_at).await?;
    replace_post_media(pool, source_kind, item).await?;
    insert_post_metric_observation(pool, submission_id, source_kind, item, observed_at).await?;
    Ok(true)
}

async fn process_media(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<bool> {
    if item.id.trim().is_empty() {
        return Ok(false);
    }

    upsert_media(pool, submission_id, source_kind, item, observed_at).await?;
    replace_media_variants(pool, source_kind, item).await?;
    Ok(true)
}

async fn upsert_actor(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO actors (
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
            pinned_post_source_ids,
            source_created_at_raw,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $17, $18, $18
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
            pinned_post_source_ids = EXCLUDED.pinned_post_source_ids,
            source_created_at_raw = EXCLUDED.source_created_at_raw,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = NOW()
        "#,
    )
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(&item.name)
    .bind(&item.screen_name)
    .bind(&item.description)
    .bind(&item.location)
    .bind(&item.avatar_url)
    .bind(trimmed_option(&item.profile_url))
    .bind(trimmed_option(&item.banner_url))
    .bind(item.is_blue_verified)
    .bind(trimmed_option(&item.verified_type))
    .bind(item.is_protected)
    .bind(&item.profile_image_shape)
    .bind(trimmed_option(&item.professional_type))
    .bind(unique_nonempty_strings(&item.pinned_tweet_ids))
    .bind(&item.created_at)
    .bind(submission_id)
    .bind(observed_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn insert_actor_profile_observation(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO actor_profile_observations (
            submission_id,
            source_kind,
            source_actor_id,
            observed_at,
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
            pinned_post_source_ids,
            source_created_at_raw
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18
        )
        ON CONFLICT (submission_id, source_kind, source_actor_id) DO UPDATE
        SET observed_at = EXCLUDED.observed_at,
            name = EXCLUDED.name,
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
            pinned_post_source_ids = EXCLUDED.pinned_post_source_ids,
            source_created_at_raw = EXCLUDED.source_created_at_raw
        "#,
    )
    .bind(submission_id)
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(observed_at)
    .bind(&item.name)
    .bind(&item.screen_name)
    .bind(&item.description)
    .bind(&item.location)
    .bind(&item.avatar_url)
    .bind(trimmed_option(&item.profile_url))
    .bind(trimmed_option(&item.banner_url))
    .bind(item.is_blue_verified)
    .bind(trimmed_option(&item.verified_type))
    .bind(item.is_protected)
    .bind(&item.profile_image_shape)
    .bind(trimmed_option(&item.professional_type))
    .bind(unique_nonempty_strings(&item.pinned_tweet_ids))
    .bind(&item.created_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn insert_actor_metric_observation(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO actor_metric_observations (
            submission_id,
            source_kind,
            source_actor_id,
            observed_at,
            followers_count,
            friends_count,
            favourites_count,
            statuses_count,
            media_count,
            listed_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (submission_id, source_kind, source_actor_id) DO UPDATE
        SET observed_at = EXCLUDED.observed_at,
            followers_count = EXCLUDED.followers_count,
            friends_count = EXCLUDED.friends_count,
            favourites_count = EXCLUDED.favourites_count,
            statuses_count = EXCLUDED.statuses_count,
            media_count = EXCLUDED.media_count,
            listed_count = EXCLUDED.listed_count
        "#,
    )
    .bind(submission_id)
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(observed_at)
    .bind(item.followers_count)
    .bind(item.friends_count)
    .bind(item.favourites_count)
    .bind(item.statuses_count)
    .bind(item.media_count)
    .bind(item.listed_count)
    .execute(pool)
    .await?;

    Ok(())
}

async fn upsert_post(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XTweetInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO posts (
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
            possibly_sensitive,
            source_label,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $16, $17, $17
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
            possibly_sensitive = EXCLUDED.possibly_sensitive,
            source_label = EXCLUDED.source_label,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = NOW()
        "#,
    )
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(item.author_id.trim())
    .bind(item.conversation_id.trim())
    .bind(&item.full_text)
    .bind(&item.legacy_full_text)
    .bind(&item.note_text)
    .bind(&item.lang)
    .bind(&item.created_at)
    .bind(trimmed_option(&item.in_reply_to_tweet_id))
    .bind(trimmed_option(&item.in_reply_to_user_id))
    .bind(trimmed_option(&item.quoted_tweet_id))
    .bind(trimmed_option(&item.retweeted_tweet_id))
    .bind(item.possibly_sensitive)
    .bind(&item.source)
    .bind(submission_id)
    .bind(observed_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn replace_post_media(pool: &PgPool, source_kind: &str, item: &XTweetInput) -> AppResult<()> {
    let post_id = item.id.trim();
    sqlx::query(
        r#"
        DELETE FROM post_media
        WHERE source_kind = $1
          AND source_post_id = $2
        "#,
    )
    .bind(source_kind)
    .bind(post_id)
    .execute(pool)
    .await?;

    let media_ids = unique_nonempty_strings(&item.media_ids);
    for (position, media_id) in media_ids.into_iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO post_media (
                source_kind,
                source_post_id,
                source_media_id,
                position
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(source_kind)
        .bind(post_id)
        .bind(media_id)
        .bind(position as i32)
        .execute(pool)
        .await?;
    }

    Ok(())
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
            submission_id,
            source_kind,
            source_post_id,
            observed_at,
            view_count,
            favorite_count,
            retweet_count,
            reply_count,
            quote_count,
            bookmark_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (submission_id, source_kind, source_post_id) DO UPDATE
        SET observed_at = EXCLUDED.observed_at,
            view_count = EXCLUDED.view_count,
            favorite_count = EXCLUDED.favorite_count,
            retweet_count = EXCLUDED.retweet_count,
            reply_count = EXCLUDED.reply_count,
            quote_count = EXCLUDED.quote_count,
            bookmark_count = EXCLUDED.bookmark_count
        "#,
    )
    .bind(submission_id)
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(observed_at)
    .bind(item.view_count)
    .bind(item.favorite_count)
    .bind(item.retweet_count)
    .bind(item.reply_count)
    .bind(item.quote_count)
    .bind(item.bookmark_count)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_media(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO media (
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
            last_observed_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $16, $17, $17
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
            updated_at = NOW()
        "#,
    )
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(&item.media_key)
    .bind(item.tweet_id.trim())
    .bind(&item.r#type)
    .bind(&item.media_url)
    .bind(&item.thumb_url)
    .bind(&item.source_url)
    .bind(item.width)
    .bind(item.height)
    .bind(&item.alt_text)
    .bind(item.allow_download)
    .bind(trimmed_option(&item.source_status_id))
    .bind(trimmed_option(&item.source_user_id))
    .bind(item.duration_ms)
    .bind(submission_id)
    .bind(observed_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn replace_media_variants(
    pool: &PgPool,
    source_kind: &str,
    item: &XMediaInput,
) -> AppResult<()> {
    let media_id = item.id.trim();
    sqlx::query(
        r#"
        DELETE FROM media_variants
        WHERE source_kind = $1
          AND source_media_id = $2
        "#,
    )
    .bind(source_kind)
    .bind(media_id)
    .execute(pool)
    .await?;

    let variants = unique_video_variants(&item.video_variants);
    for (position, variant) in variants.into_iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO media_variants (
                source_kind,
                source_media_id,
                url,
                position,
                bitrate,
                content_type
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(source_kind)
        .bind(media_id)
        .bind(variant.url)
        .bind(position as i32)
        .bind(variant.bitrate)
        .bind(variant.content_type)
        .execute(pool)
        .await?;
    }

    Ok(())
}

fn transfer_candidate(item: &XMediaInput) -> Option<(String, String)> {
    if !matches!(item.r#type.as_str(), "video" | "animated_gif")
        || item.source_url.trim().is_empty()
    {
        return None;
    }

    let content_type = item
        .video_variants
        .iter()
        .find(|variant| variant.url.trim() == item.source_url.trim())
        .map(|variant| variant.content_type.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if item.r#type == "animated_gif" {
                "image/gif".to_owned()
            } else {
                "video/mp4".to_owned()
            }
        });

    Some((item.source_url.clone(), content_type))
}

fn trimmed_option(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
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
    possibly_sensitive: Option<bool>,
    source_label: String,
}

#[derive(Debug, Clone)]
struct PostMetricRow {
    source_post_id: String,
    view_count: Option<i64>,
    favorite_count: i64,
    retweet_count: i64,
    reply_count: i64,
    quote_count: i64,
    bookmark_count: i64,
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
            possibly_sensitive,
            source_label
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
            possibly_sensitive: row.get("possibly_sensitive"),
            source_label: row.get("source_label"),
        })
        .collect())
}

async fn fetch_latest_post_metrics(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<Vec<PostMetricRow>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (source_post_id)
            source_post_id,
            view_count,
            favorite_count,
            retweet_count,
            reply_count,
            quote_count,
            bookmark_count
        FROM post_metric_observations
        WHERE source_kind = $1
          AND source_post_id = ANY($2)
        ORDER BY source_post_id, observed_at DESC, created_at DESC
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PostMetricRow {
            source_post_id: row.get("source_post_id"),
            view_count: row.get("view_count"),
            favorite_count: row.get("favorite_count"),
            retweet_count: row.get("retweet_count"),
            reply_count: row.get("reply_count"),
            quote_count: row.get("quote_count"),
            bookmark_count: row.get("bookmark_count"),
        })
        .collect())
}

async fn fetch_post_media_ids(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<HashMap<String, Vec<String>>> {
    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT source_post_id, source_media_id
        FROM post_media
        WHERE source_kind = $1
          AND source_post_id = ANY($2)
        ORDER BY source_post_id ASC, position ASC, source_media_id ASC
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    let mut media_ids_by_post: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        media_ids_by_post
            .entry(row.get("source_post_id"))
            .or_default()
            .push(row.get("source_media_id"));
    }

    Ok(media_ids_by_post)
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

fn build_post_status_aggregate(
    source_kind: &str,
    requested_id: String,
    post: Option<&PostRow>,
    actors_by_id: &HashMap<String, ActorRow>,
    media_ids: Option<&Vec<String>>,
    media_by_id: &HashMap<String, MediaRow>,
    metrics_by_post: &HashMap<String, PostMetricRow>,
    transfer_statuses: &HashMap<String, TransferStatusInfo>,
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

    let media_source_ids = media_ids.cloned().unwrap_or_default();
    let metrics = metrics_by_post.get(&post.source_post_id);

    let mut media = Vec::new();
    let mut missing_media_source_ids = Vec::new();
    let mut transfer_summary = TransferSummary::default();
    for media_id in &media_source_ids {
        if let Some(item) = media_by_id.get(media_id) {
            let transfer = transfer_statuses.get(media_id);
            match transfer.and_then(|status| status.status.as_deref()) {
                Some("pending") | Some("retryable") => transfer_summary.pending += 1,
                Some("processing") => transfer_summary.processing += 1,
                Some("succeeded") => transfer_summary.succeeded += 1,
                Some("failed") => transfer_summary.failed += 1,
                _ => {}
            }
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
                transfer_status: transfer.and_then(|status| status.status.clone()),
                storage_object_key: transfer.and_then(|status| status.storage_object_key.clone()),
                last_error: transfer.and_then(|status| status.last_error.clone()),
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
            view_count: metrics.and_then(|item| item.view_count),
            possibly_sensitive: post.possibly_sensitive,
            favorite_count: metrics.map(|item| item.favorite_count).unwrap_or_default(),
            retweet_count: metrics.map(|item| item.retweet_count).unwrap_or_default(),
            reply_count: metrics.map(|item| item.reply_count).unwrap_or_default(),
            quote_count: metrics.map(|item| item.quote_count).unwrap_or_default(),
            bookmark_count: metrics.map(|item| item.bookmark_count).unwrap_or_default(),
            media_source_ids,
            source_label: post.source_label.clone(),
        }),
        author,
        media,
        missing_media_source_ids,
        transfer_summary,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn submission_envelope_rejects_unknown_fields() {
        let payload = json!({
            "sourceKind": "x",
            "users": [],
            "tweets": [],
            "media": [],
            "captures": []
        });

        let error = serde_json::from_value::<SubmissionEnvelope>(payload).unwrap_err();
        assert!(error.to_string().contains("unknown field `captures`"));
    }

    #[test]
    fn normalize_entities_keeps_last_duplicate_and_counts_empty() {
        let first = XUserInput {
            id: "u1".to_owned(),
            name: "first".to_owned(),
            ..Default::default()
        };
        let second = XUserInput {
            id: "u1".to_owned(),
            name: "second".to_owned(),
            ..Default::default()
        };
        let empty = XUserInput::default();

        let normalized = normalize_entities(&[first, empty, second], |item| &item.id);

        assert_eq!(normalized.skipped_empty, 1);
        assert_eq!(normalized.items.len(), 1);
        assert_eq!(normalized.items[0].name, "second");
    }

    #[test]
    fn build_post_status_aggregate_uses_latest_metrics_and_media_order() {
        let post = PostRow {
            source_post_id: "p1".to_owned(),
            author_source_actor_id: "a1".to_owned(),
            conversation_source_post_id: "c1".to_owned(),
            full_text: "full".to_owned(),
            legacy_full_text: "legacy".to_owned(),
            note_text: None,
            lang: "en".to_owned(),
            source_created_at_raw: "created".to_owned(),
            in_reply_to_source_post_id: None,
            in_reply_to_source_actor_id: None,
            quoted_source_post_id: None,
            retweeted_source_post_id: None,
            possibly_sensitive: Some(false),
            source_label: "web".to_owned(),
        };

        let mut actors = HashMap::new();
        actors.insert(
            "a1".to_owned(),
            ActorRow {
                source_actor_id: "a1".to_owned(),
                name: "demo".to_owned(),
                screen_name: "demo_user".to_owned(),
                description: String::new(),
                location: String::new(),
                avatar_url: "avatar".to_owned(),
                profile_url: None,
                banner_url: None,
                verified_type: None,
            },
        );

        let media_ids = vec!["m2".to_owned(), "missing".to_owned(), "m1".to_owned()];
        let mut media_by_id = HashMap::new();
        media_by_id.insert(
            "m1".to_owned(),
            MediaRow {
                source_media_id: "m1".to_owned(),
                media_key: "mk1".to_owned(),
                source_post_id: "p1".to_owned(),
                media_type: "photo".to_owned(),
                source_url: "source1".to_owned(),
                thumb_url: "thumb1".to_owned(),
                width: 1,
                height: 1,
                alt_text: None,
                allow_download: false,
                duration_ms: None,
            },
        );
        media_by_id.insert(
            "m2".to_owned(),
            MediaRow {
                source_media_id: "m2".to_owned(),
                media_key: "mk2".to_owned(),
                source_post_id: "p1".to_owned(),
                media_type: "video".to_owned(),
                source_url: "source2".to_owned(),
                thumb_url: "thumb2".to_owned(),
                width: 2,
                height: 2,
                alt_text: Some("alt".to_owned()),
                allow_download: true,
                duration_ms: Some(20),
            },
        );

        let mut metrics = HashMap::new();
        metrics.insert(
            "p1".to_owned(),
            PostMetricRow {
                source_post_id: "p1".to_owned(),
                view_count: Some(10),
                favorite_count: 1,
                retweet_count: 2,
                reply_count: 3,
                quote_count: 4,
                bookmark_count: 5,
            },
        );

        let mut transfers = HashMap::new();
        transfers.insert(
            "m2".to_owned(),
            TransferStatusInfo {
                status: Some("processing".to_owned()),
                storage_object_key: None,
                last_error: None,
            },
        );
        transfers.insert(
            "m1".to_owned(),
            TransferStatusInfo {
                status: Some("succeeded".to_owned()),
                storage_object_key: Some("object".to_owned()),
                last_error: None,
            },
        );

        let aggregate = build_post_status_aggregate(
            "x",
            "p1".to_owned(),
            Some(&post),
            &actors,
            Some(&media_ids),
            &media_by_id,
            &metrics,
            &transfers,
        );

        assert!(aggregate.found);
        assert_eq!(aggregate.post.as_ref().unwrap().view_count, Some(10));
        assert_eq!(aggregate.post.as_ref().unwrap().media_source_ids, media_ids);
        assert_eq!(aggregate.media.len(), 2);
        assert_eq!(aggregate.media[0].source_media_id, "m2");
        assert_eq!(aggregate.media[1].source_media_id, "m1");
        assert_eq!(
            aggregate.missing_media_source_ids,
            vec!["missing".to_owned()]
        );
        assert_eq!(aggregate.transfer_summary.processing, 1);
        assert_eq!(aggregate.transfer_summary.succeeded, 1);
    }
}
