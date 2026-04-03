use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    media::{self, ManagedIdentityKind, ManagedMediaFamily, ManagedMediaSpec},
    state::AppState,
    transfer::TransferStatusInfo,
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
    pub timestamps: PostTimestampsView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostTimestampsView {
    pub post: EntityTimestampsView,
    pub metrics: Option<EntityTimestampsView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTimestampsView {
    pub last_observed_at: String,
    pub updated_at: String,
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

    let batch_outcome = match process_submission_batch(
        &state.db,
        submission_id,
        &source_kind,
        &users.items,
        &tweets.items,
        &media.items,
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
        Ok(outcome) => outcome,
        Err(error) => {
            let failure_warnings = vec![format!("ingest failed: {error}")];
            let _ = finalize_submission(
                &state.db,
                submission_id,
                "failed",
                0,
                0,
                &failure_warnings,
                now,
            )
            .await;
            return Err(error);
        }
    };
    warnings.extend(batch_outcome.warnings);
    let accepted_count = batch_outcome.accepted_count;
    let transfer_jobs_enqueued = batch_outcome.transfer_jobs_enqueued;

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

    let core_rows = fetch_post_status_core_rows(&state.db, &source_kind, &unique_post_ids).await?;
    let mut posts_by_id = HashMap::new();
    let mut actors_by_id = HashMap::new();
    let mut metrics_by_post = HashMap::new();
    for row in core_rows {
        posts_by_id.insert(row.post.source_post_id.clone(), row.post);
        if let Some(actor) = row.actor {
            actors_by_id.insert(actor.source_actor_id.clone(), actor);
        }
        if let Some(metrics) = row.metrics {
            metrics_by_post.insert(metrics.source_post_id.clone(), metrics);
        }
    }

    let media_rows =
        fetch_post_status_media_rows(&state.db, &source_kind, &unique_post_ids).await?;
    let mut media_ids_by_post: HashMap<String, Vec<String>> = HashMap::new();
    let mut media_by_id = HashMap::new();
    let mut transfer_statuses = HashMap::new();
    for row in media_rows {
        media_ids_by_post
            .entry(row.source_post_id)
            .or_default()
            .push(row.source_media_id.clone());
        if let Some(media) = row.media {
            if let Some(transfer) = row.transfer {
                transfer_statuses.insert(media.managed_media_id, transfer);
            }
            media_by_id.insert(media.source_media_id.clone(), media);
        }
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

#[derive(Debug, Default)]
struct BatchProcessOutcome {
    accepted_count: i32,
    transfer_jobs_enqueued: i32,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct PreparedMediaItem {
    item: XMediaInput,
    managed_spec: ManagedMediaSpec,
    variants: Vec<VideoVariantInput>,
}

#[derive(Debug, Clone, Serialize)]
struct ActorProfileVersionInsertRow {
    id: Uuid,
    submission_id: Uuid,
    source_kind: String,
    source_actor_id: String,
    version_no: i64,
    effective_from: OffsetDateTime,
    profile_fingerprint: String,
    name: String,
    screen_name: String,
    description: String,
    location: String,
    avatar_url: String,
    profile_url: Option<String>,
    banner_url: Option<String>,
    is_blue_verified: bool,
    verified_type: Option<String>,
    is_protected: bool,
    profile_image_shape: String,
    professional_type: Option<String>,
    pinned_post_source_ids: Vec<String>,
    source_created_at_raw: String,
    avatar_media_id: Option<Uuid>,
    banner_media_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
struct ActorCurrentProfileUpdateRow {
    source_actor_id: String,
    version_id: Uuid,
    submission_id: Uuid,
    observed_at: OffsetDateTime,
}

async fn process_submission_batch(
    pool: &PgPool,
    submission_id: Uuid,
    source_kind: &str,
    users: &[XUserInput],
    tweets: &[XTweetInput],
    media_items: &[XMediaInput],
    observed_at: OffsetDateTime,
    metrics_min_interval: time::Duration,
) -> AppResult<BatchProcessOutcome> {
    let mut outcome = BatchProcessOutcome {
        accepted_count: (users.len() + tweets.len()) as i32,
        transfer_jobs_enqueued: 0,
        warnings: Vec::new(),
    };
    let prepared_media = prepare_media_items(
        source_kind,
        submission_id,
        media_items,
        observed_at,
        &mut outcome.warnings,
    )?;
    outcome.accepted_count += prepared_media.len() as i32;

    let mut tx = pool.begin().await?;
    upsert_actor_heads_batch(&mut tx, submission_id, source_kind, users, observed_at).await?;

    let mut managed_media_specs =
        build_actor_media_specs(source_kind, submission_id, users, observed_at);
    managed_media_specs.extend(prepared_media.iter().map(|item| item.managed_spec.clone()));
    let managed_media_by_key = if managed_media_specs.is_empty() {
        HashMap::new()
    } else {
        let records = media::register_managed_media_batch(&mut tx, &managed_media_specs).await?;
        outcome.transfer_jobs_enqueued =
            records.iter().filter(|item| item.transfer_enqueued).count() as i32;
        managed_media_records_by_key(records)
    };

    sync_actor_profiles_batch(
        &mut tx,
        submission_id,
        source_kind,
        users,
        observed_at,
        &managed_media_by_key,
    )
    .await?;
    insert_actor_metrics_batch(
        &mut tx,
        submission_id,
        source_kind,
        users,
        observed_at,
        metrics_min_interval,
    )
    .await?;
    upsert_posts_batch(&mut tx, submission_id, source_kind, tweets, observed_at).await?;
    replace_post_media_batch(&mut tx, source_kind, tweets).await?;
    insert_post_metric_observations_batch(&mut tx, submission_id, source_kind, tweets, observed_at)
        .await?;
    upsert_post_media_sources_batch(
        &mut tx,
        submission_id,
        source_kind,
        &prepared_media,
        observed_at,
        &managed_media_by_key,
    )
    .await?;
    replace_media_variants_batch(&mut tx, source_kind, &prepared_media).await?;
    tx.commit().await?;

    Ok(outcome)
}

async fn upsert_actor_heads_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[XUserInput],
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    let actor_ids = items
        .iter()
        .map(|item| item.id.trim().to_owned())
        .collect::<Vec<_>>();
    if actor_ids.is_empty() {
        return Ok(());
    }

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
        SELECT $1, actor_id, $2, $2, $3, $3
        FROM UNNEST($4::text[]) AS input(actor_id)
        ON CONFLICT (source_kind, source_actor_id) DO UPDATE
        SET last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = NOW()
        "#,
    )
    .bind(source_kind)
    .bind(submission_id)
    .bind(observed_at)
    .bind(actor_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn sync_actor_profiles_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[XUserInput],
    observed_at: OffsetDateTime,
    managed_media_by_key: &HashMap<String, media::ManagedMediaBatchRecord>,
) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }

    let actor_ids = items
        .iter()
        .map(|item| item.id.trim().to_owned())
        .collect::<Vec<_>>();
    let current_versions =
        fetch_current_actor_profile_versions_batch(tx, source_kind, &actor_ids).await?;
    let mut close_ids = Vec::new();
    let mut insert_rows = Vec::new();
    let mut current_rows = Vec::new();

    for item in items {
        let actor_id = item.id.trim();
        let fingerprint = actor_profile_fingerprint(item)?;
        let current = current_versions.get(actor_id);
        if current
            .as_ref()
            .is_some_and(|row| row.profile_fingerprint == fingerprint)
        {
            continue;
        }

        if let Some(row) = current {
            close_ids.push(row.id);
        }

        let version_id = Uuid::now_v7();
        let version_no = current.map(|row| row.version_no + 1).unwrap_or(1);
        insert_rows.push(ActorProfileVersionInsertRow {
            id: version_id,
            submission_id,
            source_kind: source_kind.to_owned(),
            source_actor_id: actor_id.to_owned(),
            version_no,
            effective_from: observed_at,
            profile_fingerprint: fingerprint,
            name: item.name.trim().to_owned(),
            screen_name: item.screen_name.trim().to_owned(),
            description: item.description.trim().to_owned(),
            location: item.location.trim().to_owned(),
            avatar_url: item.avatar_url.trim().to_owned(),
            profile_url: trimmed_option(&item.profile_url),
            banner_url: trimmed_option(&item.banner_url),
            is_blue_verified: item.is_blue_verified,
            verified_type: trimmed_option(&item.verified_type),
            is_protected: item.is_protected,
            profile_image_shape: item.profile_image_shape.trim().to_owned(),
            professional_type: trimmed_option(&item.professional_type),
            pinned_post_source_ids: unique_nonempty_strings(&item.pinned_tweet_ids),
            source_created_at_raw: item.created_at.trim().to_owned(),
            avatar_media_id: actor_avatar_media_id(source_kind, item, managed_media_by_key),
            banner_media_id: actor_banner_media_id(source_kind, item, managed_media_by_key),
        });
        current_rows.push(ActorCurrentProfileUpdateRow {
            source_actor_id: actor_id.to_owned(),
            version_id,
            submission_id,
            observed_at,
        });
    }

    if !close_ids.is_empty() {
        close_actor_profile_versions_batch(tx, &close_ids, observed_at).await?;
    }
    if !insert_rows.is_empty() {
        insert_actor_profile_versions_batch(tx, &insert_rows).await?;
        set_actor_current_profile_versions_batch(tx, source_kind, &current_rows).await?;
    }
    Ok(())
}

async fn fetch_current_actor_profile_versions_batch(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    actor_ids: &[String],
) -> AppResult<HashMap<String, CurrentActorProfileVersionRow>> {
    if actor_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            a.source_actor_id,
            v.id,
            v.version_no,
            v.profile_fingerprint
        FROM actors a
        LEFT JOIN actor_profile_versions v ON v.id = a.current_profile_version_id
        WHERE a.source_kind = $1
          AND a.source_actor_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(actor_ids)
    .fetch_all(&mut **tx)
    .await?;

    let mut result = HashMap::new();
    for row in rows {
        if let Ok(id) = row.try_get("id") {
            result.insert(
                row.get::<String, _>("source_actor_id"),
                CurrentActorProfileVersionRow {
                    id,
                    version_no: row.get("version_no"),
                    profile_fingerprint: row.get("profile_fingerprint"),
                },
            );
        }
    }
    Ok(result)
}

async fn close_actor_profile_versions_batch(
    tx: &mut DbTx<'_>,
    version_ids: &[Uuid],
    effective_to: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE actor_profile_versions
        SET effective_to = $2
        WHERE id = ANY($1)
          AND effective_to IS NULL
        "#,
    )
    .bind(version_ids)
    .bind(effective_to)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_actor_profile_versions_batch(
    tx: &mut DbTx<'_>,
    rows: &[ActorProfileVersionInsertRow],
) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let payload = serde_json::to_value(rows)?;
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
        SELECT
            item.id,
            item.submission_id,
            item.source_kind,
            item.source_actor_id,
            item.version_no,
            item.effective_from,
            item.profile_fingerprint,
            item.name,
            item.screen_name,
            item.description,
            item.location,
            item.avatar_url,
            item.profile_url,
            item.banner_url,
            item.is_blue_verified,
            item.verified_type,
            item.is_protected,
            item.profile_image_shape,
            item.professional_type,
            ARRAY(
                SELECT jsonb_array_elements_text(
                    COALESCE(item.pinned_post_source_ids, '[]'::jsonb)
                )
            ),
            item.source_created_at_raw,
            item.avatar_media_id,
            item.banner_media_id
        FROM jsonb_to_recordset($1::jsonb) AS item(
            id UUID,
            submission_id UUID,
            source_kind TEXT,
            source_actor_id TEXT,
            version_no BIGINT,
            effective_from TIMESTAMPTZ,
            profile_fingerprint TEXT,
            name TEXT,
            screen_name TEXT,
            description TEXT,
            location TEXT,
            avatar_url TEXT,
            profile_url TEXT,
            banner_url TEXT,
            is_blue_verified BOOLEAN,
            verified_type TEXT,
            is_protected BOOLEAN,
            profile_image_shape TEXT,
            professional_type TEXT,
            pinned_post_source_ids JSONB,
            source_created_at_raw TEXT,
            avatar_media_id UUID,
            banner_media_id UUID
        )
        "#,
    )
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn set_actor_current_profile_versions_batch(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    rows: &[ActorCurrentProfileUpdateRow],
) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let payload = serde_json::to_value(rows)?;
    sqlx::query(
        r#"
        WITH input AS (
            SELECT
                source_actor_id,
                version_id,
                submission_id,
                observed_at
            FROM jsonb_to_recordset($2::jsonb) AS item(
                source_actor_id TEXT,
                version_id UUID,
                submission_id UUID,
                observed_at TIMESTAMPTZ
            )
        )
        UPDATE actors
        SET current_profile_version_id = input.version_id,
            last_submission_id = input.submission_id,
            last_observed_at = input.observed_at,
            updated_at = NOW()
        FROM input
        WHERE actors.source_kind = $1
          AND actors.source_actor_id = input.source_actor_id
        "#,
    )
    .bind(source_kind)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_actor_metrics_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[XUserInput],
    observed_at: OffsetDateTime,
    min_interval: time::Duration,
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct ActorMetricInsertRow {
        submission_id: Uuid,
        source_kind: String,
        source_actor_id: String,
        observed_at: OffsetDateTime,
        followers_count: i64,
        friends_count: i64,
        favourites_count: i64,
        statuses_count: i64,
        media_count: i64,
        listed_count: i64,
    }

    if items.is_empty() {
        return Ok(());
    }

    let actor_ids = items
        .iter()
        .map(|item| item.id.trim().to_owned())
        .collect::<Vec<_>>();
    let latest = fetch_latest_actor_metric_observations_batch(tx, source_kind, &actor_ids).await?;
    let rows = items
        .iter()
        .filter(|item| {
            should_insert_actor_metric_observation(
                latest.get(item.id.trim()),
                item,
                observed_at,
                min_interval,
            )
        })
        .map(|item| ActorMetricInsertRow {
            submission_id,
            source_kind: source_kind.to_owned(),
            source_actor_id: item.id.trim().to_owned(),
            observed_at,
            followers_count: item.followers_count,
            friends_count: item.friends_count,
            favourites_count: item.favourites_count,
            statuses_count: item.statuses_count,
            media_count: item.media_count,
            listed_count: item.listed_count,
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Ok(());
    }

    let payload = serde_json::to_value(&rows)?;
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
        SELECT
            item.submission_id,
            item.source_kind,
            item.source_actor_id,
            item.observed_at,
            item.followers_count,
            item.friends_count,
            item.favourites_count,
            item.statuses_count,
            item.media_count,
            item.listed_count
        FROM jsonb_to_recordset($1::jsonb) AS item(
            submission_id UUID,
            source_kind TEXT,
            source_actor_id TEXT,
            observed_at TIMESTAMPTZ,
            followers_count BIGINT,
            friends_count BIGINT,
            favourites_count BIGINT,
            statuses_count BIGINT,
            media_count BIGINT,
            listed_count BIGINT
        )
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
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn fetch_latest_actor_metric_observations_batch(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    actor_ids: &[String],
) -> AppResult<HashMap<String, LatestActorMetricObservationRow>> {
    if actor_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (source_actor_id)
            source_actor_id,
            observed_at,
            followers_count,
            friends_count,
            favourites_count,
            statuses_count,
            media_count,
            listed_count
        FROM actor_metric_observations
        WHERE source_kind = $1
          AND source_actor_id = ANY($2)
        ORDER BY source_actor_id, observed_at DESC, created_at DESC
        "#,
    )
    .bind(source_kind)
    .bind(actor_ids)
    .fetch_all(&mut **tx)
    .await?;

    let mut result = HashMap::new();
    for row in rows {
        result.insert(
            row.get::<String, _>("source_actor_id"),
            LatestActorMetricObservationRow {
                observed_at: row.get("observed_at"),
                followers_count: row.get("followers_count"),
                friends_count: row.get("friends_count"),
                favourites_count: row.get("favourites_count"),
                statuses_count: row.get("statuses_count"),
                media_count: row.get("media_count"),
                listed_count: row.get("listed_count"),
            },
        );
    }
    Ok(result)
}

async fn upsert_posts_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[XTweetInput],
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct PostUpsertRow {
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

    if items.is_empty() {
        return Ok(());
    }

    let rows = items
        .iter()
        .map(|item| PostUpsertRow {
            source_post_id: item.id.trim().to_owned(),
            author_source_actor_id: item.author_id.trim().to_owned(),
            conversation_source_post_id: item.conversation_id.trim().to_owned(),
            full_text: item.full_text.trim().to_owned(),
            legacy_full_text: item.legacy_full_text.trim().to_owned(),
            note_text: trimmed_option(&item.note_text),
            lang: item.lang.trim().to_owned(),
            source_created_at_raw: item.created_at.trim().to_owned(),
            in_reply_to_source_post_id: trimmed_option(&item.in_reply_to_tweet_id),
            in_reply_to_source_actor_id: trimmed_option(&item.in_reply_to_user_id),
            quoted_source_post_id: trimmed_option(&item.quoted_tweet_id),
            retweeted_source_post_id: trimmed_option(&item.retweeted_tweet_id),
            possibly_sensitive: item.possibly_sensitive,
            source_label: item.source.trim().to_owned(),
        })
        .collect::<Vec<_>>();
    let payload = serde_json::to_value(&rows)?;
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
        SELECT
            $1,
            item.source_post_id,
            item.author_source_actor_id,
            item.conversation_source_post_id,
            item.full_text,
            item.legacy_full_text,
            item.note_text,
            item.lang,
            item.source_created_at_raw,
            item.in_reply_to_source_post_id,
            item.in_reply_to_source_actor_id,
            item.quoted_source_post_id,
            item.retweeted_source_post_id,
            item.possibly_sensitive,
            item.source_label,
            $2,
            $2,
            $3,
            $3
        FROM jsonb_to_recordset($4::jsonb) AS item(
            source_post_id TEXT,
            author_source_actor_id TEXT,
            conversation_source_post_id TEXT,
            full_text TEXT,
            legacy_full_text TEXT,
            note_text TEXT,
            lang TEXT,
            source_created_at_raw TEXT,
            in_reply_to_source_post_id TEXT,
            in_reply_to_source_actor_id TEXT,
            quoted_source_post_id TEXT,
            retweeted_source_post_id TEXT,
            possibly_sensitive BOOLEAN,
            source_label TEXT
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
    .bind(submission_id)
    .bind(observed_at)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_post_media_batch(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    items: &[XTweetInput],
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct PostMediaRow {
        source_post_id: String,
        source_media_id: String,
        position: i32,
    }

    if items.is_empty() {
        return Ok(());
    }

    let post_ids = items
        .iter()
        .map(|item| item.id.trim().to_owned())
        .collect::<Vec<_>>();
    sqlx::query(
        r#"
        DELETE FROM post_media
        WHERE source_kind = $1
          AND source_post_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .execute(&mut **tx)
    .await?;

    let mut rows = Vec::new();
    for item in items {
        for (position, media_id) in unique_nonempty_strings(&item.media_ids)
            .into_iter()
            .enumerate()
        {
            rows.push(PostMediaRow {
                source_post_id: item.id.trim().to_owned(),
                source_media_id: media_id,
                position: position as i32,
            });
        }
    }
    if rows.is_empty() {
        return Ok(());
    }

    let payload = serde_json::to_value(&rows)?;
    sqlx::query(
        r#"
        INSERT INTO post_media (
            source_kind,
            source_post_id,
            source_media_id,
            position
        )
        SELECT
            $1,
            item.source_post_id,
            item.source_media_id,
            item.position
        FROM jsonb_to_recordset($2::jsonb) AS item(
            source_post_id TEXT,
            source_media_id TEXT,
            position INTEGER
        )
        "#,
    )
    .bind(source_kind)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_post_metric_observations_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[XTweetInput],
    observed_at: OffsetDateTime,
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct PostMetricInsertRow {
        source_post_id: String,
        view_count: Option<i64>,
        favorite_count: i64,
        retweet_count: i64,
        reply_count: i64,
        quote_count: i64,
        bookmark_count: i64,
    }

    if items.is_empty() {
        return Ok(());
    }

    let rows = items
        .iter()
        .map(|item| PostMetricInsertRow {
            source_post_id: item.id.trim().to_owned(),
            view_count: item.view_count,
            favorite_count: item.favorite_count,
            retweet_count: item.retweet_count,
            reply_count: item.reply_count,
            quote_count: item.quote_count,
            bookmark_count: item.bookmark_count,
        })
        .collect::<Vec<_>>();
    let payload = serde_json::to_value(&rows)?;
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
        SELECT
            $1,
            $2,
            item.source_post_id,
            $3,
            item.view_count,
            item.favorite_count,
            item.retweet_count,
            item.reply_count,
            item.quote_count,
            item.bookmark_count
        FROM jsonb_to_recordset($4::jsonb) AS item(
            source_post_id TEXT,
            view_count BIGINT,
            favorite_count BIGINT,
            retweet_count BIGINT,
            reply_count BIGINT,
            quote_count BIGINT,
            bookmark_count BIGINT
        )
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
    .bind(observed_at)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_post_media_sources_batch(
    tx: &mut DbTx<'_>,
    submission_id: Uuid,
    source_kind: &str,
    items: &[PreparedMediaItem],
    observed_at: OffsetDateTime,
    managed_media_by_key: &HashMap<String, media::ManagedMediaBatchRecord>,
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct PostMediaSourceUpsertRow {
        source_media_id: String,
        managed_media_id: Uuid,
        media_key: String,
        source_post_id: String,
        media_type: String,
        media_url: String,
        thumb_url: String,
        source_url: String,
        width: i32,
        height: i32,
        alt_text: Option<String>,
        allow_download: bool,
        source_status_id: Option<String>,
        source_actor_id: Option<String>,
        duration_ms: Option<i64>,
    }

    if items.is_empty() {
        return Ok(());
    }

    let mut rows = Vec::new();
    for item in items {
        let managed_media = managed_media_by_key
            .get(&managed_media_identity_key(
                &item.managed_spec.source_kind,
                item.managed_spec.identity_kind.as_str(),
                &item.managed_spec.identity_value,
            ))
            .ok_or_else(|| {
                AppError::upstream("missing managed media result for post media source")
            })?;
        rows.push(PostMediaSourceUpsertRow {
            source_media_id: item.item.id.trim().to_owned(),
            managed_media_id: managed_media.id,
            media_key: item.item.media_key.trim().to_owned(),
            source_post_id: item.item.tweet_id.trim().to_owned(),
            media_type: item.item.r#type.trim().to_owned(),
            media_url: item.item.media_url.trim().to_owned(),
            thumb_url: item.item.thumb_url.trim().to_owned(),
            source_url: item.item.source_url.trim().to_owned(),
            width: item.item.width,
            height: item.item.height,
            alt_text: trimmed_option(&item.item.alt_text),
            allow_download: item.item.allow_download,
            source_status_id: trimmed_option(&item.item.source_status_id),
            source_actor_id: trimmed_option(&item.item.source_user_id),
            duration_ms: item.item.duration_ms,
        });
    }

    let payload = serde_json::to_value(&rows)?;
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
        SELECT
            $1,
            item.source_media_id,
            item.managed_media_id,
            item.media_key,
            item.source_post_id,
            item.media_type,
            item.media_url,
            item.thumb_url,
            item.source_url,
            item.width,
            item.height,
            item.alt_text,
            item.allow_download,
            item.source_status_id,
            item.source_actor_id,
            item.duration_ms,
            $2,
            $2,
            $3,
            $3
        FROM jsonb_to_recordset($4::jsonb) AS item(
            source_media_id TEXT,
            managed_media_id UUID,
            media_key TEXT,
            source_post_id TEXT,
            media_type TEXT,
            media_url TEXT,
            thumb_url TEXT,
            source_url TEXT,
            width INTEGER,
            height INTEGER,
            alt_text TEXT,
            allow_download BOOLEAN,
            source_status_id TEXT,
            source_actor_id TEXT,
            duration_ms BIGINT
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
    .bind(submission_id)
    .bind(observed_at)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_media_variants_batch(
    tx: &mut DbTx<'_>,
    source_kind: &str,
    items: &[PreparedMediaItem],
) -> AppResult<()> {
    #[derive(Debug, Clone, Serialize)]
    struct MediaVariantInsertRow {
        source_media_id: String,
        url: String,
        position: i32,
        bitrate: Option<i64>,
        content_type: String,
    }

    if items.is_empty() {
        return Ok(());
    }

    let media_ids = items
        .iter()
        .map(|item| item.item.id.trim().to_owned())
        .collect::<Vec<_>>();
    sqlx::query(
        r#"
        DELETE FROM media_variants
        WHERE source_kind = $1
          AND source_media_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(media_ids)
    .execute(&mut **tx)
    .await?;

    let mut rows = Vec::new();
    for item in items {
        for (position, variant) in item.variants.iter().enumerate() {
            rows.push(MediaVariantInsertRow {
                source_media_id: item.item.id.trim().to_owned(),
                url: variant.url.clone(),
                position: position as i32,
                bitrate: variant.bitrate,
                content_type: variant.content_type.trim().to_owned(),
            });
        }
    }
    if rows.is_empty() {
        return Ok(());
    }

    let payload = serde_json::to_value(&rows)?;
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
        SELECT
            $1,
            item.source_media_id,
            item.url,
            item.position,
            item.bitrate,
            item.content_type
        FROM jsonb_to_recordset($2::jsonb) AS item(
            source_media_id TEXT,
            url TEXT,
            position INTEGER,
            bitrate BIGINT,
            content_type TEXT
        )
        "#,
    )
    .bind(source_kind)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn build_actor_media_specs(
    source_kind: &str,
    submission_id: Uuid,
    items: &[XUserInput],
    observed_at: OffsetDateTime,
) -> Vec<ManagedMediaSpec> {
    let mut specs = Vec::new();
    for item in items {
        if let Some(fetch_url) = media::normalize_actor_avatar_fetch_url(&item.avatar_url) {
            specs.push(ManagedMediaSpec {
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
            });
        }

        if let Some(raw_banner_url) = trimmed_option(&item.banner_url) {
            if let Some(fetch_url) = media::normalize_actor_banner_fetch_url(&raw_banner_url) {
                specs.push(ManagedMediaSpec {
                    source_kind: source_kind.to_owned(),
                    media_family: ManagedMediaFamily::Image,
                    identity_kind: ManagedIdentityKind::ActorBannerUrl,
                    identity_value: fetch_url.clone(),
                    fetch_url,
                    display_url: raw_banner_url,
                    thumb_url: None,
                    content_type_hint: media::infer_content_type_from_url(
                        item.banner_url.as_deref().unwrap_or_default(),
                    ),
                    submission_id,
                    observed_at,
                });
            }
        }
    }
    specs
}

fn prepare_media_items(
    source_kind: &str,
    submission_id: Uuid,
    items: &[XMediaInput],
    observed_at: OffsetDateTime,
    warnings: &mut Vec<String>,
) -> AppResult<Vec<PreparedMediaItem>> {
    let mut prepared = Vec::new();
    for item in items {
        match build_post_media_spec(source_kind, submission_id, item, observed_at) {
            Ok((managed_spec, variants)) => prepared.push(PreparedMediaItem {
                item: item.clone(),
                managed_spec,
                variants,
            }),
            Err(error) => warnings.push(format!("failed to upsert media {}: {error}", item.id)),
        }
    }
    Ok(prepared)
}

fn build_post_media_spec(
    source_kind: &str,
    submission_id: Uuid,
    item: &XMediaInput,
    observed_at: OffsetDateTime,
) -> AppResult<(ManagedMediaSpec, Vec<VideoVariantInput>)> {
    let fetch_url = media::normalize_post_source_url(&item.source_url).ok_or_else(|| {
        AppError::bad_request(format!("media {} sourceUrl is required", item.id.trim()))
    })?;
    let display_url = if item.media_url.trim().is_empty() {
        fetch_url.clone()
    } else {
        item.media_url.trim().to_owned()
    };

    Ok((
        ManagedMediaSpec {
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
        },
        unique_video_variants(&item.video_variants),
    ))
}

fn actor_avatar_media_id(
    source_kind: &str,
    item: &XUserInput,
    managed_media_by_key: &HashMap<String, media::ManagedMediaBatchRecord>,
) -> Option<Uuid> {
    let fetch_url = media::normalize_actor_avatar_fetch_url(&item.avatar_url)?;
    managed_media_by_key
        .get(&managed_media_identity_key(
            source_kind,
            ManagedIdentityKind::ActorAvatarUrl.as_str(),
            &fetch_url,
        ))
        .map(|item| item.id)
}

fn actor_banner_media_id(
    source_kind: &str,
    item: &XUserInput,
    managed_media_by_key: &HashMap<String, media::ManagedMediaBatchRecord>,
) -> Option<Uuid> {
    let raw_banner_url = trimmed_option(&item.banner_url)?;
    let fetch_url = media::normalize_actor_banner_fetch_url(&raw_banner_url)?;
    managed_media_by_key
        .get(&managed_media_identity_key(
            source_kind,
            ManagedIdentityKind::ActorBannerUrl.as_str(),
            &fetch_url,
        ))
        .map(|item| item.id)
}

fn managed_media_records_by_key(
    records: Vec<media::ManagedMediaBatchRecord>,
) -> HashMap<String, media::ManagedMediaBatchRecord> {
    records
        .into_iter()
        .map(|item| {
            (
                managed_media_identity_key(
                    &item.source_kind,
                    &item.identity_kind,
                    &item.identity_value,
                ),
                item,
            )
        })
        .collect()
}

fn managed_media_identity_key(
    source_kind: &str,
    identity_kind: &str,
    identity_value: &str,
) -> String {
    format!("{source_kind}\n{identity_kind}\n{identity_value}")
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
    last_observed_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
struct PostMetricRow {
    source_post_id: String,
    observed_at: OffsetDateTime,
    created_at: OffsetDateTime,
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

#[derive(Debug, Clone)]
struct PostStatusCoreRow {
    post: PostRow,
    actor: Option<ActorRow>,
    metrics: Option<PostMetricRow>,
}

#[derive(Debug, Clone)]
struct PostStatusMediaRow {
    source_post_id: String,
    source_media_id: String,
    media: Option<MediaRow>,
    transfer: Option<TransferStatusInfo>,
}

async fn fetch_post_status_core_rows(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<Vec<PostStatusCoreRow>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            p.source_post_id AS post_source_post_id,
            p.author_source_actor_id AS post_author_source_actor_id,
            p.conversation_source_post_id AS post_conversation_source_post_id,
            p.full_text AS post_full_text,
            p.legacy_full_text AS post_legacy_full_text,
            p.note_text AS post_note_text,
            p.lang AS post_lang,
            p.source_created_at_raw AS post_source_created_at_raw,
            p.in_reply_to_source_post_id AS post_in_reply_to_source_post_id,
            p.in_reply_to_source_actor_id AS post_in_reply_to_source_actor_id,
            p.quoted_source_post_id AS post_quoted_source_post_id,
            p.retweeted_source_post_id AS post_retweeted_source_post_id,
            p.possibly_sensitive AS post_possibly_sensitive,
            p.source_label AS post_source_label,
            p.last_observed_at AS post_last_observed_at,
            p.updated_at AS post_updated_at,
            actor.source_actor_id AS actor_source_actor_id,
            actor.name AS actor_name,
            actor.screen_name AS actor_screen_name,
            actor.description AS actor_description,
            actor.location AS actor_location,
            actor.avatar_url AS actor_avatar_url,
            actor.profile_url AS actor_profile_url,
            actor.banner_url AS actor_banner_url,
            actor.verified_type AS actor_verified_type,
            metrics.source_post_id AS metric_source_post_id,
            metrics.observed_at AS metric_observed_at,
            metrics.created_at AS metric_created_at,
            metrics.view_count AS metric_view_count,
            metrics.favorite_count AS metric_favorite_count,
            metrics.retweet_count AS metric_retweet_count,
            metrics.reply_count AS metric_reply_count,
            metrics.quote_count AS metric_quote_count,
            metrics.bookmark_count AS metric_bookmark_count
        FROM posts p
        LEFT JOIN LATERAL (
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
            INNER JOIN actor_profile_versions v ON v.id = a.current_profile_version_id
            WHERE a.source_kind = p.source_kind
              AND a.source_actor_id = p.author_source_actor_id
        ) actor ON true
        LEFT JOIN LATERAL (
            SELECT
                source_post_id,
                observed_at,
                created_at,
                view_count,
                favorite_count,
                retweet_count,
                reply_count,
                quote_count,
                bookmark_count
            FROM post_metric_observations
            WHERE source_kind = p.source_kind
              AND source_post_id = p.source_post_id
            ORDER BY observed_at DESC, created_at DESC
            LIMIT 1
        ) metrics ON true
        WHERE p.source_kind = $1
          AND p.source_post_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let actor_source_actor_id = row.get::<Option<String>, _>("actor_source_actor_id");
            let metric_source_post_id = row.get::<Option<String>, _>("metric_source_post_id");

            PostStatusCoreRow {
                post: PostRow {
                    source_post_id: row.get("post_source_post_id"),
                    author_source_actor_id: row.get("post_author_source_actor_id"),
                    conversation_source_post_id: row.get("post_conversation_source_post_id"),
                    full_text: row.get("post_full_text"),
                    legacy_full_text: row.get("post_legacy_full_text"),
                    note_text: row.get("post_note_text"),
                    lang: row.get("post_lang"),
                    source_created_at_raw: row.get("post_source_created_at_raw"),
                    in_reply_to_source_post_id: row.get("post_in_reply_to_source_post_id"),
                    in_reply_to_source_actor_id: row.get("post_in_reply_to_source_actor_id"),
                    quoted_source_post_id: row.get("post_quoted_source_post_id"),
                    retweeted_source_post_id: row.get("post_retweeted_source_post_id"),
                    possibly_sensitive: row.get("post_possibly_sensitive"),
                    source_label: row.get("post_source_label"),
                    last_observed_at: row.get("post_last_observed_at"),
                    updated_at: row.get("post_updated_at"),
                },
                actor: actor_source_actor_id.map(|source_actor_id| ActorRow {
                    source_actor_id,
                    name: row.get("actor_name"),
                    screen_name: row.get("actor_screen_name"),
                    description: row.get("actor_description"),
                    location: row.get("actor_location"),
                    avatar_url: row.get("actor_avatar_url"),
                    profile_url: row.get("actor_profile_url"),
                    banner_url: row.get("actor_banner_url"),
                    verified_type: row.get("actor_verified_type"),
                }),
                metrics: metric_source_post_id.map(|source_post_id| PostMetricRow {
                    source_post_id,
                    observed_at: row.get("metric_observed_at"),
                    created_at: row.get("metric_created_at"),
                    view_count: row.get("metric_view_count"),
                    favorite_count: row.get("metric_favorite_count"),
                    retweet_count: row.get("metric_retweet_count"),
                    reply_count: row.get("metric_reply_count"),
                    quote_count: row.get("metric_quote_count"),
                    bookmark_count: row.get("metric_bookmark_count"),
                }),
            }
        })
        .collect())
}

async fn fetch_post_status_media_rows(
    pool: &PgPool,
    source_kind: &str,
    post_ids: &[String],
) -> AppResult<Vec<PostStatusMediaRow>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            pm.source_post_id,
            pm.source_media_id,
            pms.source_media_id AS media_source_media_id,
            pms.managed_media_id AS media_managed_media_id,
            pms.media_key AS media_media_key,
            pms.source_post_id AS media_source_post_id,
            pms.media_type AS media_media_type,
            pms.source_url AS media_source_url,
            pms.thumb_url AS media_thumb_url,
            pms.width AS media_width,
            pms.height AS media_height,
            pms.alt_text AS media_alt_text,
            pms.allow_download AS media_allow_download,
            pms.duration_ms AS media_duration_ms,
            j.status AS transfer_status,
            j.last_error AS transfer_last_error,
            o.object_key AS transfer_storage_object_key
        FROM post_media pm
        LEFT JOIN post_media_sources pms
            ON pms.source_kind = pm.source_kind
           AND pms.source_media_id = pm.source_media_id
        LEFT JOIN media_transfer_jobs j ON j.media_id = pms.managed_media_id
        LEFT JOIN media_storage_bindings b
            ON b.media_id = pms.managed_media_id
           AND b.object_role = 'original'
        LEFT JOIN storage_objects o ON o.id = b.storage_object_id
        WHERE pm.source_kind = $1
          AND pm.source_post_id = ANY($2)
        ORDER BY pm.source_post_id ASC, pm.position ASC, pm.source_media_id ASC
        "#,
    )
    .bind(source_kind)
    .bind(post_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let media_source_media_id = row.get::<Option<String>, _>("media_source_media_id");
            let transfer_status = row.get::<Option<String>, _>("transfer_status");

            PostStatusMediaRow {
                source_post_id: row.get("source_post_id"),
                source_media_id: row.get("source_media_id"),
                media: media_source_media_id.map(|source_media_id| MediaRow {
                    source_media_id,
                    managed_media_id: row.get("media_managed_media_id"),
                    media_key: row.get("media_media_key"),
                    source_post_id: row.get("media_source_post_id"),
                    media_type: row.get("media_media_type"),
                    source_url: row.get("media_source_url"),
                    thumb_url: row.get("media_thumb_url"),
                    width: row.get("media_width"),
                    height: row.get("media_height"),
                    alt_text: row.get("media_alt_text"),
                    allow_download: row.get("media_allow_download"),
                    duration_ms: row.get("media_duration_ms"),
                }),
                transfer: transfer_status.map(|status| TransferStatusInfo {
                    status: Some(status),
                    storage_object_key: row.get("transfer_storage_object_key"),
                    last_error: row.get("transfer_last_error"),
                }),
            }
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
            timestamps: PostTimestampsView {
                post: build_entity_timestamps(post.last_observed_at, post.updated_at),
                metrics: metrics
                    .map(|item| build_entity_timestamps(item.observed_at, item.created_at)),
            },
        }),
        author,
        media,
        missing_media_source_ids,
        transfer_summary,
    }
}

fn build_entity_timestamps(
    last_observed_at: OffsetDateTime,
    updated_at: OffsetDateTime,
) -> EntityTimestampsView {
    EntityTimestampsView {
        last_observed_at: format_time(last_observed_at),
        updated_at: format_time(updated_at),
    }
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;
    use time::macros::datetime;

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
        let post_last_observed_at = datetime!(2026-04-03 12:34:56 UTC);
        let post_updated_at = datetime!(2026-04-03 12:35:02 UTC);
        let metrics_observed_at = datetime!(2026-04-03 12:36:00 UTC);
        let metrics_updated_at = datetime!(2026-04-03 12:36:05 UTC);
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
            last_observed_at: post_last_observed_at,
            updated_at: post_updated_at,
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
                observed_at: metrics_observed_at,
                created_at: metrics_updated_at,
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
        assert_eq!(
            aggregate
                .post
                .as_ref()
                .unwrap()
                .timestamps
                .post
                .last_observed_at,
            "2026-04-03T12:34:56Z"
        );
        assert_eq!(
            aggregate.post.as_ref().unwrap().timestamps.post.updated_at,
            "2026-04-03T12:35:02Z"
        );
        assert_eq!(
            aggregate
                .post
                .as_ref()
                .unwrap()
                .timestamps
                .metrics
                .as_ref()
                .unwrap()
                .last_observed_at,
            "2026-04-03T12:36:00Z"
        );
        assert_eq!(
            aggregate
                .post
                .as_ref()
                .unwrap()
                .timestamps
                .metrics
                .as_ref()
                .unwrap()
                .updated_at,
            "2026-04-03T12:36:05Z"
        );
        assert_eq!(aggregate.media.len(), 2);
        assert_eq!(aggregate.media[0].source_media_id, "m2");
        assert_eq!(aggregate.media[1].source_media_id, "m1");
        assert_eq!(
            aggregate.missing_media_source_ids,
            vec!["missing".to_owned()]
        );
        assert_eq!(aggregate.transfer_summary.processing, 1);
        assert_eq!(aggregate.transfer_summary.succeeded, 1);

        let value = serde_json::to_value(&aggregate).unwrap();
        assert_eq!(
            value["post"]["timestamps"]["post"]["lastObservedAt"],
            "2026-04-03T12:34:56Z"
        );
        assert_eq!(
            value["post"]["timestamps"]["post"]["updatedAt"],
            "2026-04-03T12:35:02Z"
        );
        assert_eq!(
            value["post"]["timestamps"]["metrics"]["lastObservedAt"],
            "2026-04-03T12:36:00Z"
        );
        assert_eq!(
            value["post"]["timestamps"]["metrics"]["updatedAt"],
            "2026-04-03T12:36:05Z"
        );
    }

    #[test]
    fn build_post_status_aggregate_returns_null_metric_timestamps_without_metrics() {
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
            last_observed_at: datetime!(2026-04-03 13:00:00 UTC),
            updated_at: datetime!(2026-04-03 13:00:05 UTC),
        };

        let aggregate = build_post_status_aggregate(
            "x",
            "p1".to_owned(),
            Some(&post),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert!(aggregate.found);
        assert!(
            aggregate
                .post
                .as_ref()
                .unwrap()
                .timestamps
                .metrics
                .is_none()
        );

        let value = serde_json::to_value(&aggregate).unwrap();
        assert_eq!(
            value["post"]["timestamps"]["metrics"],
            serde_json::Value::Null
        );
    }
}
