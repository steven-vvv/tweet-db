use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    media::{self, ManagedIdentityKind, ManagedMediaFamily, ManagedMediaSpec},
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

#[derive(Debug, Default)]
struct ProcessOutcome {
    accepted: bool,
    transfer_jobs_enqueued: i32,
}

type DbTx<'a> = Transaction<'a, Postgres>;

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
        match process_actor(
            &state.db,
            submission_id,
            &source_kind,
            user,
            now,
            time::Duration::seconds(
                state
                    .settings
                    .config
                    .ingest
                    .actor_metrics_min_interval_seconds,
            ),
        )
        .await
        {
            Ok(outcome) if outcome.accepted => {
                accepted_count += 1;
                transfer_jobs_enqueued += outcome.transfer_jobs_enqueued;
            }
            Ok(_) => warnings.push("skipped user with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert user {}: {error}", user.id)),
        }
    }

    for tweet in &tweets.items {
        match process_post(&state.db, submission_id, &source_kind, tweet, now).await {
            Ok(outcome) if outcome.accepted => accepted_count += 1,
            Ok(_) => warnings.push("skipped tweet with empty id".to_owned()),
            Err(error) => warnings.push(format!("failed to upsert tweet {}: {error}", tweet.id)),
        }
    }

    for item in &media.items {
        match process_media(&state.db, submission_id, &source_kind, item, now).await {
            Ok(outcome) if outcome.accepted => {
                accepted_count += 1;
                transfer_jobs_enqueued += outcome.transfer_jobs_enqueued;
            }
            Ok(_) => warnings.push("skipped media with empty id".to_owned()),
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

    let managed_media_ids = media_by_id
        .values()
        .map(|item| item.managed_media_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let transfer_statuses =
        transfer::fetch_transfer_statuses(&state.db, &managed_media_ids).await?;

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
    metrics_min_interval: time::Duration,
) -> AppResult<ProcessOutcome> {
    let actor_id = item.id.trim();
    if actor_id.is_empty() {
        return Ok(ProcessOutcome::default());
    }

    let mut tx = pool.begin().await?;

    upsert_actor_head(&mut tx, submission_id, source_kind, actor_id, observed_at).await?;
    let avatar_media =
        register_actor_avatar_media(&mut tx, submission_id, source_kind, item, observed_at).await?;
    let banner_media =
        register_actor_banner_media(&mut tx, submission_id, source_kind, item, observed_at).await?;

    sync_actor_profile_version(
        &mut tx,
        submission_id,
        source_kind,
        item,
        avatar_media.as_ref().map(|item| item.id),
        banner_media.as_ref().map(|item| item.id),
        observed_at,
    )
    .await?;

    insert_actor_metric_observation(
        &mut tx,
        submission_id,
        source_kind,
        item,
        observed_at,
        metrics_min_interval,
    )
    .await?;
    tx.commit().await?;

    Ok(ProcessOutcome {
        accepted: true,
        transfer_jobs_enqueued: i32::from(
            avatar_media
                .as_ref()
                .map(|item| item.transfer_enqueued)
                .unwrap_or(false),
        ) + i32::from(
            banner_media
                .as_ref()
                .map(|item| item.transfer_enqueued)
                .unwrap_or(false),
        ),
    })
}

async fn process_post(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XTweetInput,
    observed_at: OffsetDateTime,
) -> AppResult<ProcessOutcome> {
    if item.id.trim().is_empty() {
        return Ok(ProcessOutcome::default());
    }

    let mut tx = pool.begin().await?;
    upsert_post(&mut tx, submission_id, source_kind, item, observed_at).await?;
    replace_post_media(&mut tx, source_kind, item).await?;
    insert_post_metric_observation(&mut tx, submission_id, source_kind, item, observed_at).await?;
    tx.commit().await?;

    Ok(ProcessOutcome {
        accepted: true,
        transfer_jobs_enqueued: 0,
    })
}

async fn process_media(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<ProcessOutcome> {
    if item.id.trim().is_empty() {
        return Ok(ProcessOutcome::default());
    }

    let mut tx = pool.begin().await?;
    let managed_media =
        register_post_media(&mut tx, submission_id, source_kind, item, observed_at).await?;
    upsert_post_media_source(
        &mut tx,
        submission_id,
        source_kind,
        item,
        managed_media.id,
        observed_at,
    )
    .await?;
    replace_media_variants(&mut tx, source_kind, item).await?;
    tx.commit().await?;

    Ok(ProcessOutcome {
        accepted: true,
        transfer_jobs_enqueued: i32::from(managed_media.transfer_enqueued),
    })
}

async fn upsert_actor_head(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    actor_id: &str,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO actors (
            source_kind,
            source_actor_id,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at
        )
        VALUES ($1, $2, $3, $3, $4, $4)
        ON CONFLICT (source_kind, source_actor_id) DO UPDATE
        SET last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = NOW()
        "#,
    )
    .bind(source_kind)
    .bind(actor_id)
    .bind(submission_id)
    .bind(observed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn sync_actor_profile_version(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    avatar_media_id: Option<Uuid>,
    banner_media_id: Option<Uuid>,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    let actor_id = item.id.trim();
    let fingerprint = actor_profile_fingerprint(item)?;
    let current = fetch_current_actor_profile_version(tx, source_kind, actor_id).await?;

    if let Some(current) = current.as_ref() {
        if current.profile_fingerprint == fingerprint {
            return Ok(());
        }

        close_actor_profile_version(tx, current.id, observed_at).await?;
    }

    let version_id = Uuid::now_v7();
    let version_no = current.map(|item| item.version_no + 1).unwrap_or(1);
    insert_actor_profile_version(
        tx,
        version_id,
        submission_id,
        source_kind,
        item,
        version_no,
        &fingerprint,
        avatar_media_id,
        banner_media_id,
        observed_at,
    )
    .await?;
    set_actor_current_profile_version(
        tx,
        source_kind,
        actor_id,
        version_id,
        submission_id,
        observed_at,
    )
    .await?;

    Ok(())
}

async fn fetch_current_actor_profile_version(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    actor_id: &str,
) -> AppResult<Option<CurrentActorProfileVersionRow>> {
    let row = sqlx::query(
        r#"
        SELECT
            v.id,
            v.version_no,
            v.profile_fingerprint
        FROM actors a
        LEFT JOIN actor_profile_versions v ON v.id = a.current_profile_version_id
        WHERE a.source_kind = $1
          AND a.source_actor_id = $2
        "#,
    )
    .bind(source_kind)
    .bind(actor_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.and_then(|row| {
        row.try_get("id")
            .ok()
            .map(|id| CurrentActorProfileVersionRow {
                id,
                version_no: row.get("version_no"),
                profile_fingerprint: row.get("profile_fingerprint"),
            })
    }))
}

async fn close_actor_profile_version(
    tx: &mut DbTx<'_>,
    version_id: Uuid,
    effective_to: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE actor_profile_versions
        SET effective_to = $2
        WHERE id = $1
          AND effective_to IS NULL
        "#,
    )
    .bind(version_id)
    .bind(effective_to)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_actor_profile_version(
    tx: &mut DbTx<'_>,
    version_id: Uuid,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    version_no: i64,
    fingerprint: &str,
    avatar_media_id: Option<Uuid>,
    banner_media_id: Option<Uuid>,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO actor_profile_versions (
            id,
            submission_id,
            source_kind,
            source_actor_id,
            version_no,
            effective_from,
            profile_fingerprint,
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
            avatar_media_id,
            banner_media_id
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23
        )
        "#,
    )
    .bind(version_id)
    .bind(submission_id)
    .bind(source_kind)
    .bind(item.id.trim())
    .bind(version_no)
    .bind(observed_at)
    .bind(fingerprint)
    .bind(item.name.trim())
    .bind(item.screen_name.trim())
    .bind(item.description.trim())
    .bind(item.location.trim())
    .bind(item.avatar_url.trim())
    .bind(trimmed_option(&item.profile_url))
    .bind(trimmed_option(&item.banner_url))
    .bind(item.is_blue_verified)
    .bind(trimmed_option(&item.verified_type))
    .bind(item.is_protected)
    .bind(item.profile_image_shape.trim())
    .bind(trimmed_option(&item.professional_type))
    .bind(unique_nonempty_strings(&item.pinned_tweet_ids))
    .bind(item.created_at.trim())
    .bind(avatar_media_id)
    .bind(banner_media_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn set_actor_current_profile_version(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    actor_id: &str,
    version_id: Uuid,
    submission_id: Uuid,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE actors
        SET current_profile_version_id = $3,
            last_submission_id = $4,
            last_observed_at = $5,
            updated_at = NOW()
        WHERE source_kind = $1
          AND source_actor_id = $2
        "#,
    )
    .bind(source_kind)
    .bind(actor_id)
    .bind(version_id)
    .bind(submission_id)
    .bind(observed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn insert_actor_metric_observation(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
    min_interval: time::Duration,
) -> AppResult<()> {
    let latest = fetch_latest_actor_metric_observation(tx, source_kind, item.id.trim()).await?;
    if !should_insert_actor_metric_observation(latest.as_ref(), item, observed_at, min_interval) {
        return Ok(());
    }

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
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn fetch_latest_actor_metric_observation(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    actor_id: &str,
) -> AppResult<Option<LatestActorMetricObservationRow>> {
    let row = sqlx::query(
        r#"
        SELECT
            observed_at,
            followers_count,
            friends_count,
            favourites_count,
            statuses_count,
            media_count,
            listed_count
        FROM actor_metric_observations
        WHERE source_kind = $1
          AND source_actor_id = $2
        ORDER BY observed_at DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .bind(source_kind)
    .bind(actor_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(|row| LatestActorMetricObservationRow {
        observed_at: row.get("observed_at"),
        followers_count: row.get("followers_count"),
        friends_count: row.get("friends_count"),
        favourites_count: row.get("favourites_count"),
        statuses_count: row.get("statuses_count"),
        media_count: row.get("media_count"),
        listed_count: row.get("listed_count"),
    }))
}

fn should_insert_actor_metric_observation(
    latest: Option<&LatestActorMetricObservationRow>,
    item: &XUserInput,
    observed_at: OffsetDateTime,
    min_interval: time::Duration,
) -> bool {
    let Some(latest) = latest else {
        return true;
    };

    actor_metrics_changed(latest, item) || observed_at - latest.observed_at >= min_interval
}

fn actor_metrics_changed(latest: &LatestActorMetricObservationRow, item: &XUserInput) -> bool {
    latest.followers_count != item.followers_count
        || latest.friends_count != item.friends_count
        || latest.favourites_count != item.favourites_count
        || latest.statuses_count != item.statuses_count
        || latest.media_count != item.media_count
        || latest.listed_count != item.listed_count
}

async fn upsert_post(
    tx: &mut DbTx<'_>,
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
    .bind(item.full_text.trim())
    .bind(item.legacy_full_text.trim())
    .bind(trimmed_option(&item.note_text))
    .bind(item.lang.trim())
    .bind(item.created_at.trim())
    .bind(trimmed_option(&item.in_reply_to_tweet_id))
    .bind(trimmed_option(&item.in_reply_to_user_id))
    .bind(trimmed_option(&item.quoted_tweet_id))
    .bind(trimmed_option(&item.retweeted_tweet_id))
    .bind(item.possibly_sensitive)
    .bind(item.source.trim())
    .bind(submission_id)
    .bind(observed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn replace_post_media(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    item: &XTweetInput,
) -> AppResult<()> {
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
    .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_post_metric_observation(
    tx: &mut DbTx<'_>,
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn register_post_media(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<media::ManagedMediaRecord> {
    let fetch_url = media::normalize_post_source_url(&item.source_url).ok_or_else(|| {
        AppError::bad_request(format!("media {} sourceUrl is required", item.id.trim()))
    })?;
    let display_url = if item.media_url.trim().is_empty() {
        fetch_url.clone()
    } else {
        item.media_url.trim().to_owned()
    };

    let spec = ManagedMediaSpec {
        source_kind: source_kind.to_owned(),
        media_family: media_family_for_type(&item.r#type)?,
        identity_kind: ManagedIdentityKind::PostSourceUrl,
        identity_value: fetch_url.clone(),
        fetch_url,
        display_url,
        thumb_url: trimmed_option(&Some(item.thumb_url.clone())),
        content_type_hint: media_content_type_hint(item),
        submission_id,
        observed_at,
    };

    media::register_managed_media(tx, &spec).await
}

async fn register_actor_avatar_media(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<Option<media::ManagedMediaRecord>> {
    let Some(fetch_url) = media::normalize_actor_avatar_fetch_url(&item.avatar_url) else {
        return Ok(None);
    };

    let spec = ManagedMediaSpec {
        source_kind: source_kind.to_owned(),
        media_family: ManagedMediaFamily::Image,
        identity_kind: ManagedIdentityKind::ActorAvatarUrl,
        identity_value: fetch_url.clone(),
        fetch_url,
        display_url: item.avatar_url.trim().to_owned(),
        thumb_url: None,
        content_type_hint: media::infer_content_type_from_url(&item.avatar_url),
        submission_id,
        observed_at,
    };

    media::register_managed_media(tx, &spec).await.map(Some)
}

async fn register_actor_banner_media(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XUserInput,
    observed_at: OffsetDateTime,
) -> AppResult<Option<media::ManagedMediaRecord>> {
    let Some(raw_banner_url) = trimmed_option(&item.banner_url) else {
        return Ok(None);
    };
    let Some(fetch_url) = media::normalize_actor_banner_fetch_url(&raw_banner_url) else {
        return Ok(None);
    };

    let spec = ManagedMediaSpec {
        source_kind: source_kind.to_owned(),
        media_family: ManagedMediaFamily::Image,
        identity_kind: ManagedIdentityKind::ActorBannerUrl,
        identity_value: fetch_url.clone(),
        fetch_url,
        display_url: raw_banner_url.clone(),
        thumb_url: None,
        content_type_hint: media::infer_content_type_from_url(&raw_banner_url),
        submission_id,
        observed_at,
    };

    media::register_managed_media(tx, &spec).await.map(Some)
}

async fn upsert_post_media_source(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    item: &XMediaInput,
    managed_media_id: Uuid,
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO post_media_sources (
            source_kind,
            source_media_id,
            managed_media_id,
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
            $11, $12, $13, $14, $15, $16, $17, $17, $18, $18
        )
        ON CONFLICT (source_kind, source_media_id) DO UPDATE
        SET managed_media_id = EXCLUDED.managed_media_id,
            media_key = EXCLUDED.media_key,
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
    .bind(managed_media_id)
    .bind(item.media_key.trim())
    .bind(item.tweet_id.trim())
    .bind(item.r#type.trim())
    .bind(item.media_url.trim())
    .bind(item.thumb_url.trim())
    .bind(item.source_url.trim())
    .bind(item.width)
    .bind(item.height)
    .bind(trimmed_option(&item.alt_text))
    .bind(item.allow_download)
    .bind(trimmed_option(&item.source_status_id))
    .bind(trimmed_option(&item.source_user_id))
    .bind(item.duration_ms)
    .bind(submission_id)
    .bind(observed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn replace_media_variants(
    tx: &mut DbTx<'_>,
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
    .execute(&mut **tx)
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
        .bind(variant.content_type.trim())
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn media_family_for_type(value: &str) -> AppResult<ManagedMediaFamily> {
    match value.trim() {
        "photo" => Ok(ManagedMediaFamily::Image),
        "video" => Ok(ManagedMediaFamily::Video),
        "animated_gif" => Ok(ManagedMediaFamily::AnimatedGif),
        other => Err(AppError::bad_request(format!(
            "unsupported media type `{other}`"
        ))),
    }
}

fn media_content_type_hint(item: &XMediaInput) -> Option<String> {
    let source_url = item.source_url.trim();
    if source_url.is_empty() {
        return None;
    }

    if let Some(value) = item
        .video_variants
        .iter()
        .find(|variant| variant.url.trim() == source_url)
        .map(|variant| variant.content_type.trim())
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_owned());
    }

    media::infer_content_type_from_url(source_url)
        .or_else(|| media::infer_content_type_from_url(&item.media_url))
        .or_else(|| match item.r#type.trim() {
            "photo" => Some("image/jpeg".to_owned()),
            "video" => Some("video/mp4".to_owned()),
            "animated_gif" => Some("video/mp4".to_owned()),
            _ => None,
        })
}

fn actor_profile_fingerprint(item: &XUserInput) -> AppResult<String> {
    #[derive(Serialize)]
    struct FingerprintPayload<'a> {
        name: &'a str,
        screen_name: &'a str,
        description: &'a str,
        location: &'a str,
        avatar_url: &'a str,
        profile_url: Option<String>,
        banner_url: Option<String>,
        is_blue_verified: bool,
        verified_type: Option<String>,
        is_protected: bool,
        profile_image_shape: &'a str,
        professional_type: Option<String>,
        pinned_post_source_ids: Vec<String>,
        source_created_at_raw: &'a str,
    }

    let payload = FingerprintPayload {
        name: item.name.trim(),
        screen_name: item.screen_name.trim(),
        description: item.description.trim(),
        location: item.location.trim(),
        avatar_url: item.avatar_url.trim(),
        profile_url: trimmed_option(&item.profile_url),
        banner_url: trimmed_option(&item.banner_url),
        is_blue_verified: item.is_blue_verified,
        verified_type: trimmed_option(&item.verified_type),
        is_protected: item.is_protected,
        profile_image_shape: item.profile_image_shape.trim(),
        professional_type: trimmed_option(&item.professional_type),
        pinned_post_source_ids: unique_nonempty_strings(&item.pinned_tweet_ids),
        source_created_at_raw: item.created_at.trim(),
    };

    let bytes = serde_json::to_vec(&payload)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn trimmed_option(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Clone)]
struct CurrentActorProfileVersionRow {
    id: Uuid,
    version_no: i64,
    profile_fingerprint: String,
}

#[derive(Debug, Clone)]
struct LatestActorMetricObservationRow {
    observed_at: OffsetDateTime,
    followers_count: i64,
    friends_count: i64,
    favourites_count: i64,
    statuses_count: i64,
    media_count: i64,
    listed_count: i64,
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
    managed_media_id: Uuid,
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
            a.source_actor_id,
            v.name,
            v.screen_name,
            v.description,
            v.location,
            v.avatar_url,
            v.profile_url,
            v.banner_url,
            v.verified_type
        FROM actors a
        JOIN actor_profile_versions v ON v.id = a.current_profile_version_id
        WHERE a.source_kind = $1
          AND a.source_actor_id = ANY($2)
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
            managed_media_id,
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
        FROM post_media_sources
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
            managed_media_id: row.get("managed_media_id"),
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
    transfer_statuses: &HashMap<Uuid, TransferStatusInfo>,
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
            let transfer = transfer_statuses.get(&item.managed_media_id);
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
    fn actor_profile_fingerprint_ignores_metric_only_changes() {
        let base = XUserInput {
            id: "u1".to_owned(),
            name: "demo".to_owned(),
            screen_name: "demo_user".to_owned(),
            avatar_url: "https://example.com/avatar_normal.jpg".to_owned(),
            banner_url: Some("https://example.com/banner".to_owned()),
            followers_count: 10,
            ..Default::default()
        };
        let mut changed = base.clone();
        changed.followers_count = 999;
        changed.statuses_count = 77;

        let left = actor_profile_fingerprint(&base).unwrap();
        let right = actor_profile_fingerprint(&changed).unwrap();
        assert_eq!(left, right);

        changed.avatar_url = "https://example.com/avatar2_normal.jpg".to_owned();
        let changed_avatar = actor_profile_fingerprint(&changed).unwrap();
        assert_ne!(left, changed_avatar);
    }

    #[test]
    fn actor_metric_observation_requires_change_or_interval() {
        let observed_at = OffsetDateTime::now_utc();
        let latest = LatestActorMetricObservationRow {
            observed_at: observed_at - time::Duration::hours(1),
            followers_count: 10,
            friends_count: 20,
            favourites_count: 30,
            statuses_count: 40,
            media_count: 50,
            listed_count: 60,
        };
        let item = XUserInput {
            id: "u1".to_owned(),
            followers_count: 10,
            friends_count: 20,
            favourites_count: 30,
            statuses_count: 40,
            media_count: 50,
            listed_count: 60,
            ..Default::default()
        };

        assert!(!should_insert_actor_metric_observation(
            Some(&latest),
            &item,
            observed_at,
            time::Duration::hours(24),
        ));

        let mut changed = item.clone();
        changed.followers_count = 11;
        assert!(should_insert_actor_metric_observation(
            Some(&latest),
            &changed,
            observed_at,
            time::Duration::hours(24),
        ));

        let older = LatestActorMetricObservationRow {
            observed_at: observed_at - time::Duration::hours(24),
            ..latest
        };
        assert!(should_insert_actor_metric_observation(
            Some(&older),
            &item,
            observed_at,
            time::Duration::hours(24),
        ));
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

        let managed_1 = Uuid::now_v7();
        let managed_2 = Uuid::now_v7();
        let media_ids = vec!["m2".to_owned(), "missing".to_owned(), "m1".to_owned()];
        let mut media_by_id = HashMap::new();
        media_by_id.insert(
            "m1".to_owned(),
            MediaRow {
                source_media_id: "m1".to_owned(),
                managed_media_id: managed_1,
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
                managed_media_id: managed_2,
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
            managed_2,
            TransferStatusInfo {
                status: Some("processing".to_owned()),
                storage_object_key: None,
                last_error: None,
            },
        );
        transfers.insert(
            managed_1,
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
