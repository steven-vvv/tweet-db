use std::{cmp, collections::HashMap};

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Clone)]
struct TransferJob {
    id: Uuid,
    source_kind: String,
    source_media_id: String,
    source_post_id: String,
    source_url: String,
    content_type: String,
    attempt_count: i32,
}

#[derive(Debug, Clone, Default)]
pub struct TransferStatusInfo {
    pub status: Option<String>,
    pub storage_object_key: Option<String>,
    pub last_error: Option<String>,
}

pub fn spawn_worker(state: AppState) {
    if !state.settings.config.transfer.enabled {
        return;
    }
    if state.settings.secrets.storage_access_key.is_none()
        || state.settings.secrets.storage_secret_key.is_none()
    {
        tracing::warn!("transfer worker disabled because storage credentials are not configured");
        return;
    }

    tokio::spawn(async move {
        let client = match build_s3_client(&state).await {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!("transfer worker failed to build S3 client: {error}");
                return;
            }
        };

        let poll_interval = std::time::Duration::from_secs(
            state.settings.config.transfer.worker_poll_interval_seconds,
        );
        loop {
            match claim_next_job(&state.db).await {
                Ok(Some(job)) => {
                    if let Err(error) = process_job(&state, &client, job).await {
                        tracing::warn!("transfer job processing failed: {error}");
                    }
                }
                Ok(None) => tokio::time::sleep(poll_interval).await,
                Err(error) => {
                    tracing::warn!("transfer worker poll failed: {error}");
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    });
}

pub async fn enqueue_media_transfer(
    pool: &PgPool,
    source_kind: &str,
    source_media_id: &str,
    source_post_id: &str,
    source_url: &str,
    content_type: &str,
) -> AppResult<bool> {
    if source_media_id.trim().is_empty() || source_url.trim().is_empty() {
        return Ok(false);
    }

    let result = sqlx::query(
        r#"
        INSERT INTO transfer_jobs (
            id,
            source_kind,
            source_media_id,
            source_post_id,
            source_url,
            content_type,
            status,
            next_run_at,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'pending', NOW(), NOW(), NOW())
        ON CONFLICT (source_kind, source_media_id) DO UPDATE
        SET source_post_id = EXCLUDED.source_post_id,
            source_url = EXCLUDED.source_url,
            content_type = EXCLUDED.content_type,
            status = CASE
                WHEN transfer_jobs.status = 'succeeded' THEN transfer_jobs.status
                ELSE 'pending'
            END,
            next_run_at = CASE
                WHEN transfer_jobs.status = 'succeeded' THEN transfer_jobs.next_run_at
                ELSE NOW()
            END,
            updated_at = NOW()
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(source_kind)
    .bind(source_media_id)
    .bind(source_post_id)
    .bind(source_url)
    .bind(content_type)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn fetch_transfer_statuses(
    pool: &PgPool,
    source_kind: &str,
    media_ids: &[String],
) -> AppResult<HashMap<String, TransferStatusInfo>> {
    if media_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            j.source_media_id,
            j.status,
            j.last_error,
            o.object_key
        FROM transfer_jobs j
        LEFT JOIN media_storage_bindings b
            ON b.source_kind = j.source_kind
           AND b.source_media_id = j.source_media_id
           AND b.variant_role = 'primary'
        LEFT JOIN storage_objects o ON o.id = b.storage_object_id
        WHERE j.source_kind = $1
          AND j.source_media_id = ANY($2)
        "#,
    )
    .bind(source_kind)
    .bind(media_ids)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.get::<String, _>("source_media_id"),
            TransferStatusInfo {
                status: row.get("status"),
                storage_object_key: row.get("object_key"),
                last_error: row.get("last_error"),
            },
        );
    }
    Ok(map)
}

async fn build_s3_client(state: &AppState) -> AppResult<Client> {
    let access_key = state
        .settings
        .secrets
        .storage_access_key
        .clone()
        .ok_or_else(|| AppError::config("STORAGE_ACCESS_KEY is required"))?;
    let secret_key = state
        .settings
        .secrets
        .storage_secret_key
        .clone()
        .ok_or_else(|| AppError::config("STORAGE_SECRET_KEY is required"))?;
    let credentials = Credentials::new(access_key, secret_key, None, None, "tweet-db");
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(
            state.settings.config.storage.region.clone(),
        ))
        .credentials_provider(credentials)
        .load()
        .await;
    let config = aws_sdk_s3::config::Builder::from(&shared_config)
        .endpoint_url(state.settings.config.storage.endpoint.clone())
        .force_path_style(state.settings.config.storage.path_style)
        .build();
    Ok(Client::from_conf(config))
}

async fn claim_next_job(pool: &PgPool) -> AppResult<Option<TransferJob>> {
    let row = sqlx::query(
        r#"
        WITH picked AS (
            SELECT id
            FROM transfer_jobs
            WHERE status IN ('pending', 'retryable')
              AND next_run_at <= NOW()
            ORDER BY next_run_at ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE transfer_jobs j
        SET status = 'processing',
            leased_at = NOW(),
            updated_at = NOW()
        FROM picked
        WHERE j.id = picked.id
        RETURNING
            j.id,
            j.source_kind,
            j.source_media_id,
            j.source_post_id,
            j.source_url,
            j.content_type,
            j.attempt_count
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| TransferJob {
        id: row.get("id"),
        source_kind: row.get("source_kind"),
        source_media_id: row.get("source_media_id"),
        source_post_id: row.get("source_post_id"),
        source_url: row.get("source_url"),
        content_type: row.get("content_type"),
        attempt_count: row.get("attempt_count"),
    }))
}

async fn process_job(state: &AppState, client: &Client, job: TransferJob) -> AppResult<()> {
    let attempt_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO transfer_attempts (id, job_id, status, started_at)
        VALUES ($1, $2, 'running', NOW())
        "#,
    )
    .bind(attempt_id)
    .bind(job.id)
    .execute(&state.db)
    .await?;

    let result = upload_job(state, client, &job).await;
    match result {
        Ok(outcome) => {
            let storage_object_id = upsert_storage_object(
                &state.db,
                &state.settings.config.storage.provider,
                &state.settings.config.storage.bucket,
                &outcome.object_key,
                outcome.etag,
                outcome.size_bytes,
                &job.content_type,
            )
            .await?;
            bind_storage_object(
                &state.db,
                &job.source_kind,
                &job.source_media_id,
                storage_object_id,
            )
            .await?;

            sqlx::query(
                r#"
                UPDATE transfer_jobs
                SET status = 'succeeded',
                    attempt_count = attempt_count + 1,
                    storage_object_id = $2,
                    leased_at = NULL,
                    last_error = NULL,
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(job.id)
            .bind(storage_object_id)
            .execute(&state.db)
            .await?;

            sqlx::query(
                r#"
                UPDATE transfer_attempts
                SET status = 'succeeded',
                    bytes_uploaded = $2,
                    parts_uploaded = $3,
                    finished_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(outcome.size_bytes)
            .bind(outcome.parts_uploaded)
            .execute(&state.db)
            .await?;
        }
        Err(error) => {
            let next_attempt = job.attempt_count + 1;
            let terminal = next_attempt >= state.settings.config.transfer.max_attempts;
            let backoff_seconds = cmp::min(3600_i64, 30_i64 * (1_i64 << cmp::min(next_attempt, 6)));
            let next_run_at = OffsetDateTime::now_utc() + time::Duration::seconds(backoff_seconds);
            let status = if terminal { "failed" } else { "retryable" };
            let error_text = error.to_string();

            sqlx::query(
                r#"
                UPDATE transfer_jobs
                SET status = $2,
                    attempt_count = attempt_count + 1,
                    next_run_at = $3,
                    leased_at = NULL,
                    last_error = $4,
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(job.id)
            .bind(status)
            .bind(next_run_at)
            .bind(&error_text)
            .execute(&state.db)
            .await?;

            sqlx::query(
                r#"
                UPDATE transfer_attempts
                SET status = 'failed',
                    error = $2,
                    finished_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(error_text)
            .execute(&state.db)
            .await?;
        }
    }

    Ok(())
}

struct UploadOutcome {
    object_key: String,
    etag: Option<String>,
    size_bytes: i64,
    parts_uploaded: i32,
}

async fn upload_job(
    state: &AppState,
    client: &Client,
    job: &TransferJob,
) -> AppResult<UploadOutcome> {
    let object_key = build_object_key(job)?;
    let create = client
        .create_multipart_upload()
        .bucket(&state.settings.config.storage.bucket)
        .key(&object_key)
        .content_type(&job.content_type)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to create multipart upload: {error}"))
        })?;
    let upload_id = create
        .upload_id()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::upstream("multipart upload id missing"))?;

    let response = state.http_client.get(&job.source_url).send().await?;
    if !response.status().is_success() {
        let _ = abort_upload(client, state, &object_key, &upload_id).await;
        return Err(AppError::upstream(format!(
            "source download failed with status {}",
            response.status()
        )));
    }

    let mut stream = response.bytes_stream();
    let part_size = state.settings.config.transfer.part_size_bytes;
    let mut buffer = Vec::with_capacity(part_size);
    let mut parts = Vec::new();
    let mut part_number = 1_i32;
    let mut total_bytes = 0_i64;

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| AppError::upstream(format!("source stream failed: {error}")))?;
        total_bytes += chunk.len() as i64;
        buffer.extend_from_slice(&chunk);

        while buffer.len() >= part_size {
            let tail = buffer.split_off(part_size);
            let current = std::mem::replace(&mut buffer, tail);
            let part =
                upload_part(client, state, &object_key, &upload_id, part_number, current).await?;
            parts.push(part);
            part_number += 1;
        }
    }

    if !buffer.is_empty() {
        let part = upload_part(client, state, &object_key, &upload_id, part_number, buffer).await?;
        parts.push(part);
    }

    if parts.is_empty() {
        let _ = abort_upload(client, state, &object_key, &upload_id).await;
        return Err(AppError::upstream("multipart upload produced no parts"));
    }

    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(parts.clone()))
        .build();

    let complete = client
        .complete_multipart_upload()
        .bucket(&state.settings.config.storage.bucket)
        .key(&object_key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to complete multipart upload: {error}"))
        })?;

    Ok(UploadOutcome {
        object_key,
        etag: complete.e_tag().map(ToOwned::to_owned),
        size_bytes: total_bytes,
        parts_uploaded: parts.len() as i32,
    })
}

async fn upload_part(
    client: &Client,
    state: &AppState,
    object_key: &str,
    upload_id: &str,
    part_number: i32,
    bytes: Vec<u8>,
) -> AppResult<CompletedPart> {
    let response = client
        .upload_part()
        .bucket(&state.settings.config.storage.bucket)
        .key(object_key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(ByteStream::from(bytes))
        .send()
        .await
        .map_err(|error| AppError::upstream(format!("failed to upload multipart part: {error}")))?;

    Ok(CompletedPart::builder()
        .part_number(part_number)
        .set_e_tag(response.e_tag().map(ToOwned::to_owned))
        .build())
}

async fn abort_upload(
    client: &Client,
    state: &AppState,
    object_key: &str,
    upload_id: &str,
) -> AppResult<()> {
    client
        .abort_multipart_upload()
        .bucket(&state.settings.config.storage.bucket)
        .key(object_key)
        .upload_id(upload_id)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to abort multipart upload: {error}"))
        })?;
    Ok(())
}

async fn upsert_storage_object(
    pool: &PgPool,
    provider: &str,
    bucket: &str,
    object_key: &str,
    etag: Option<String>,
    size_bytes: i64,
    content_type: &str,
) -> AppResult<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO storage_objects (
            id, provider, bucket, object_key, etag, size_bytes, content_type
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (object_key) DO UPDATE
        SET etag = EXCLUDED.etag,
            size_bytes = EXCLUDED.size_bytes,
            content_type = EXCLUDED.content_type
        RETURNING id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(provider)
    .bind(bucket)
    .bind(object_key)
    .bind(etag)
    .bind(size_bytes)
    .bind(content_type)
    .fetch_one(pool)
    .await?;
    Ok(row.get("id"))
}

async fn bind_storage_object(
    pool: &PgPool,
    source_kind: &str,
    source_media_id: &str,
    storage_object_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO media_storage_bindings (
            id, source_kind, source_media_id, storage_object_id, variant_role
        )
        VALUES ($1, $2, $3, $4, 'primary')
        ON CONFLICT (source_kind, source_media_id, variant_role) DO UPDATE
        SET storage_object_id = EXCLUDED.storage_object_id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(source_kind)
    .bind(source_media_id)
    .bind(storage_object_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn build_object_key(job: &TransferJob) -> AppResult<String> {
    let now = OffsetDateTime::now_utc();
    let digest = hex_digest(&job.source_url);
    let ext = extension_for_object(&job.source_url, &job.content_type);
    Ok(format!(
        "{}/{}/{:02}/{:02}/{}/{}/{}.{}",
        job.source_kind,
        now.year(),
        u8::from(now.month()),
        now.day(),
        sanitize_key_segment(&job.source_post_id),
        sanitize_key_segment(&job.source_media_id),
        digest,
        ext
    ))
}

fn extension_for_object(source_url: &str, content_type: &str) -> String {
    if let Ok(url) = Url::parse(source_url) {
        if let Some(last) = url.path_segments().and_then(|segments| segments.last()) {
            if let Some(ext) = last.rsplit('.').next() {
                if ext != last && !ext.is_empty() {
                    return ext.to_ascii_lowercase();
                }
            }
        }
    }

    match content_type {
        "video/mp4" => "mp4".to_owned(),
        "image/gif" => "gif".to_owned(),
        _ => "bin".to_owned(),
    }
}

fn sanitize_key_segment(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "unknown".to_owned();
    }
    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn hex_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
