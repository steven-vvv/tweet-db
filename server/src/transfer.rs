use std::{cmp, collections::HashMap, sync::Arc, time::Duration as StdDuration};

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
};
use futures_util::{Stream, StreamExt, stream};
use reqwest::{
    Client as HttpClient, Response, StatusCode,
    header::{CONTENT_RANGE, CONTENT_TYPE, RANGE},
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use url::Url;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Clone)]
struct TransferJob {
    id: Uuid,
    media_id: Uuid,
    source_kind: String,
    fetch_url: String,
    content_type_hint: String,
    attempt_count: i32,
    reclaimed_from_lease: bool,
}

#[derive(Clone)]
struct TransferRuntime {
    s3_client: Client,
    buffer_permits: Arc<Semaphore>,
}

#[derive(Debug, Clone, Default)]
pub struct TransferStatusInfo {
    pub status: Option<String>,
    pub storage_object_key: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum UploadMode {
    SinglePut,
    Multipart,
}

impl UploadMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::SinglePut => "single_put",
            Self::Multipart => "multipart",
        }
    }
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
        let runtime = match build_transfer_runtime(&state).await {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!("transfer worker failed to initialize runtime: {error}");
                return;
            }
        };

        for worker_id in 0..state.settings.config.transfer.worker_count.max(1) {
            let state = state.clone();
            let runtime = runtime.clone();
            tokio::spawn(async move {
                run_worker_loop(state, runtime, worker_id).await;
            });
        }
    });
}

pub async fn enqueue_media_transfer(
    tx: &mut Transaction<'_, Postgres>,
    media_id: Uuid,
    source_kind: &str,
    fetch_url: &str,
    content_type_hint: Option<&str>,
) -> AppResult<bool> {
    let fetch_url = fetch_url.trim();
    if fetch_url.is_empty() {
        return Ok(false);
    }

    let existing = sqlx::query(
        r#"
        SELECT status, fetch_url
        FROM media_transfer_jobs
        WHERE media_id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(&mut **tx)
    .await?;

    let should_enqueue = match existing.as_ref() {
        None => true,
        Some(row) => {
            let status: String = row.get("status");
            let existing_fetch_url: String = row.get("fetch_url");
            status != "succeeded" || existing_fetch_url != fetch_url
        }
    };

    if existing.is_none() {
        sqlx::query(
            r#"
            INSERT INTO media_transfer_jobs (
                id,
                media_id,
                source_kind,
                fetch_url,
                content_type_hint,
                status,
                next_run_at,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', NOW(), NOW(), NOW())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(media_id)
        .bind(source_kind)
        .bind(fetch_url)
        .bind(content_type_hint.unwrap_or_default().trim())
        .execute(&mut **tx)
        .await?;
        return Ok(true);
    }

    let status = if should_enqueue {
        "pending"
    } else {
        "succeeded"
    };
    sqlx::query(
        r#"
        UPDATE media_transfer_jobs
        SET source_kind = $2,
            fetch_url = $3,
            content_type_hint = CASE
                WHEN $4 = '' THEN content_type_hint
                ELSE $4
            END,
            status = $5,
            next_run_at = CASE
                WHEN $5 = 'pending' THEN NOW()
                ELSE next_run_at
            END,
            leased_at = NULL,
            lease_expires_at = NULL,
            last_error = NULL,
            updated_at = NOW()
        WHERE media_id = $1
        "#,
    )
    .bind(media_id)
    .bind(source_kind)
    .bind(fetch_url)
    .bind(content_type_hint.unwrap_or_default().trim())
    .bind(status)
    .execute(&mut **tx)
    .await?;

    Ok(should_enqueue)
}

pub async fn fetch_transfer_statuses(
    pool: &PgPool,
    media_ids: &[Uuid],
) -> AppResult<HashMap<Uuid, TransferStatusInfo>> {
    if media_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            j.media_id,
            j.status,
            j.last_error,
            o.object_key
        FROM media_transfer_jobs j
        LEFT JOIN media_storage_bindings b
            ON b.media_id = j.media_id
           AND b.object_role = 'original'
        LEFT JOIN storage_objects o ON o.id = b.storage_object_id
        WHERE j.media_id = ANY($1)
        "#,
    )
    .bind(media_ids)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.get::<Uuid, _>("media_id"),
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

async fn build_transfer_runtime(state: &AppState) -> AppResult<TransferRuntime> {
    Ok(TransferRuntime {
        s3_client: build_s3_client(state).await?,
        buffer_permits: Arc::new(Semaphore::new(buffer_permit_count(state))),
    })
}

async fn run_worker_loop(state: AppState, runtime: TransferRuntime, worker_id: usize) {
    let poll_interval =
        StdDuration::from_secs(state.settings.config.transfer.worker_poll_interval_seconds);

    loop {
        match claim_next_job(&state.db, lease_duration(&state)).await {
            Ok(Some(job)) => {
                if let Err(error) = process_job(&state, &runtime, job).await {
                    tracing::warn!("transfer worker {worker_id} processing failed: {error}");
                }
            }
            Ok(None) => tokio::time::sleep(poll_interval).await,
            Err(error) => {
                tracing::warn!("transfer worker {worker_id} poll failed: {error}");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

async fn claim_next_job(
    pool: &PgPool,
    lease_duration: time::Duration,
) -> AppResult<Option<TransferJob>> {
    let lease_expires_at = OffsetDateTime::now_utc() + lease_duration;
    let row = sqlx::query(
        r#"
        WITH picked AS (
            SELECT id, status AS previous_status
            FROM media_transfer_jobs
            WHERE (
                    status IN ('pending', 'retryable')
                AND next_run_at <= NOW()
            ) OR (
                    status = 'processing'
                AND lease_expires_at IS NOT NULL
                AND lease_expires_at <= NOW()
            )
            ORDER BY COALESCE(lease_expires_at, next_run_at) ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE media_transfer_jobs j
        SET status = 'processing',
            leased_at = NOW(),
            lease_expires_at = $1,
            updated_at = NOW()
        FROM picked
        WHERE j.id = picked.id
        RETURNING
            j.id,
            j.media_id,
            j.source_kind,
            j.fetch_url,
            j.content_type_hint,
            j.attempt_count,
            picked.previous_status
        "#,
    )
    .bind(lease_expires_at)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let job = TransferJob {
        id: row.get("id"),
        media_id: row.get("media_id"),
        source_kind: row.get("source_kind"),
        fetch_url: row.get("fetch_url"),
        content_type_hint: row.get("content_type_hint"),
        attempt_count: row.get("attempt_count"),
        reclaimed_from_lease: row.get::<String, _>("previous_status") == "processing",
    };
    if job.reclaimed_from_lease {
        mark_stale_attempts_failed(pool, job.id).await?;
    }

    Ok(Some(job))
}

async fn mark_stale_attempts_failed(pool: &PgPool, job_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE media_transfer_attempts
        SET status = 'failed',
            error = COALESCE(error, 'worker lease expired'),
            finished_at = COALESCE(finished_at, NOW())
        WHERE job_id = $1
          AND status = 'running'
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn process_job(
    state: &AppState,
    runtime: &TransferRuntime,
    job: TransferJob,
) -> AppResult<()> {
    let attempt_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO media_transfer_attempts (
            id, job_id, status, upload_mode, started_at
        )
        VALUES ($1, $2, 'running', 'single_put', NOW())
        "#,
    )
    .bind(attempt_id)
    .bind(job.id)
    .execute(&state.db)
    .await?;

    let attempt_timeout_seconds = state.settings.config.transfer.attempt_timeout_seconds;
    let result = match tokio::time::timeout(
        StdDuration::from_secs(attempt_timeout_seconds),
        upload_job(state, runtime, &job),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(AppError::upstream(format!(
            "transfer attempt timed out after {attempt_timeout_seconds} seconds"
        ))),
    };
    match result {
        Ok(outcome) => {
            let storage_object_id = upsert_storage_object(
                &state.db,
                &state.settings.config.storage.provider,
                &state.settings.config.storage.bucket,
                &outcome.object_key,
                outcome.etag,
                outcome.size_bytes,
                &outcome.content_type,
            )
            .await?;
            bind_storage_object(&state.db, job.media_id, storage_object_id).await?;

            sqlx::query(
                r#"
                UPDATE media_transfer_jobs
                SET status = 'succeeded',
                    attempt_count = attempt_count + 1,
                    storage_object_id = $2,
                    leased_at = NULL,
                    lease_expires_at = NULL,
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
                UPDATE media_transfer_attempts
                SET status = 'succeeded',
                    upload_mode = $2,
                    bytes_uploaded = $3,
                    parts_uploaded = $4,
                    finished_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(outcome.upload_mode.as_str())
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
                UPDATE media_transfer_jobs
                SET status = $2,
                    attempt_count = attempt_count + 1,
                    next_run_at = $3,
                    leased_at = NULL,
                    lease_expires_at = NULL,
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
                UPDATE media_transfer_attempts
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
    content_type: String,
    upload_mode: UploadMode,
}

async fn upload_job(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
) -> AppResult<UploadOutcome> {
    let chunk_size = state.settings.config.transfer.chunk_size_bytes.max(1);
    let response = state
        .transfer_http_client
        .get(&job.fetch_url)
        .header(RANGE, range_header_value(0, chunk_size as u64 - 1))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "source download failed with status {}",
            response.status()
        )));
    }

    let content_type = response_content_type(&response, &job.content_type_hint);
    let first_buffer_permit = acquire_buffer_permit(runtime).await?;
    if response.status() == StatusCode::PARTIAL_CONTENT {
        return upload_from_ranged_probe(
            state,
            runtime,
            job,
            response,
            &content_type,
            chunk_size,
            first_buffer_permit,
        )
        .await;
    }

    upload_from_full_response(
        state,
        runtime,
        job,
        response,
        &content_type,
        chunk_size,
        first_buffer_permit,
    )
    .await
}

async fn upload_from_ranged_probe(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    response: Response,
    content_type: &str,
    chunk_size: usize,
    first_buffer_permit: OwnedSemaphorePermit,
) -> AppResult<UploadOutcome> {
    let total_size = parse_total_size_from_content_range(&response);
    let first_part = response
        .bytes()
        .await
        .map_err(|error| AppError::upstream(format!("failed to read ranged probe: {error}")))?
        .to_vec();

    if first_part.len() < chunk_size {
        return upload_single_object(
            state,
            runtime,
            job,
            first_part,
            first_buffer_permit,
            content_type,
        )
        .await;
    }

    if let Some(total_size) = total_size {
        if total_size < chunk_size as u64 {
            return upload_single_object(
                state,
                runtime,
                job,
                first_part,
                first_buffer_permit,
                content_type,
            )
            .await;
        }

        return upload_multipart_parallel_from_ranges(
            state,
            runtime,
            job,
            first_part,
            first_buffer_permit,
            total_size,
            content_type,
            chunk_size,
        )
        .await;
    }

    upload_multipart_from_unknown_ranges(
        state,
        runtime,
        job,
        first_part,
        first_buffer_permit,
        content_type,
        chunk_size,
    )
    .await
}

async fn upload_from_full_response(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    response: Response,
    content_type: &str,
    chunk_size: usize,
    first_buffer_permit: OwnedSemaphorePermit,
) -> AppResult<UploadOutcome> {
    let mut stream = response.bytes_stream();
    let mut first_buffer = Vec::with_capacity(chunk_size);

    while first_buffer.len() < chunk_size {
        match stream.next().await {
            Some(chunk) => {
                let chunk = chunk.map_err(|error| {
                    AppError::upstream(format!("source stream failed during probe: {error}"))
                })?;
                first_buffer.extend_from_slice(&chunk);
            }
            None => {
                return upload_single_object(
                    state,
                    runtime,
                    job,
                    first_buffer,
                    first_buffer_permit,
                    content_type,
                )
                .await;
            }
        }
    }

    let remainder = first_buffer.split_off(chunk_size);
    upload_multipart_from_stream(
        state,
        runtime,
        job,
        first_buffer,
        first_buffer_permit,
        remainder,
        stream,
        content_type,
        chunk_size,
    )
    .await
}

async fn upload_single_object(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    bytes: Vec<u8>,
    _buffer_permit: OwnedSemaphorePermit,
    content_type: &str,
) -> AppResult<UploadOutcome> {
    let object_key = build_object_key(job, content_type)?;
    let response = runtime
        .s3_client
        .put_object()
        .bucket(&state.settings.config.storage.bucket)
        .key(&object_key)
        .content_type(content_type)
        .body(ByteStream::from(bytes.clone()))
        .send()
        .await
        .map_err(|error| AppError::upstream(format!("failed to upload object: {error}")))?;

    Ok(UploadOutcome {
        object_key,
        etag: response.e_tag().map(ToOwned::to_owned),
        size_bytes: bytes.len() as i64,
        parts_uploaded: 1,
        content_type: content_type.to_owned(),
        upload_mode: UploadMode::SinglePut,
    })
}

async fn upload_multipart_parallel_from_ranges(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    first_part: Vec<u8>,
    _first_part_permit: OwnedSemaphorePermit,
    total_size: u64,
    content_type: &str,
    chunk_size: usize,
) -> AppResult<UploadOutcome> {
    let object_key = build_object_key(job, content_type)?;
    let upload_id =
        create_multipart_upload(state, &runtime.s3_client, &object_key, content_type).await?;
    let bucket = state.settings.config.storage.bucket.clone();
    let mut completed_parts = Vec::new();

    let first_completed = match upload_part(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        1,
        first_part,
    )
    .await
    {
        Ok(part) => part,
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    };
    drop(_first_part_permit);
    completed_parts.push(NumberedCompletedPart {
        part_number: 1,
        part: first_completed,
    });

    let total_parts = total_size.div_ceil(chunk_size as u64) as i32;
    let in_flight = state.settings.config.transfer.max_in_flight_parts.max(1);
    let download_limit = Arc::new(Semaphore::new(
        state.settings.config.transfer.download_parallelism.max(1),
    ));
    let upload_limit = Arc::new(Semaphore::new(
        state.settings.config.transfer.upload_parallelism.max(1),
    ));

    let fetch_url = job.fetch_url.clone();
    let http_client = state.transfer_http_client.clone();
    let upload_client = runtime.s3_client.clone();
    let buffer_permits = runtime.buffer_permits.clone();

    let mut tasks = stream::iter(2..=total_parts)
        .map(|part_number| {
            let fetch_url = fetch_url.clone();
            let http_client = http_client.clone();
            let upload_client = upload_client.clone();
            let bucket = bucket.clone();
            let object_key = object_key.clone();
            let upload_id = upload_id.clone();
            let download_limit = download_limit.clone();
            let upload_limit = upload_limit.clone();
            let buffer_permits = buffer_permits.clone();

            async move {
                let _download_permit = download_limit
                    .acquire_owned()
                    .await
                    .map_err(|_| AppError::upstream("download semaphore closed"))?;
                let start = ((part_number - 1) as u64) * chunk_size as u64;
                let end = cmp::min(total_size, start + chunk_size as u64) - 1;
                let _buffer_permit = buffer_permits
                    .acquire_owned()
                    .await
                    .map_err(|_| AppError::upstream("buffer semaphore closed"))?;
                let bytes = download_range_bytes(&http_client, &fetch_url, start, end).await?;
                drop(_download_permit);

                let _upload_permit = upload_limit
                    .acquire_owned()
                    .await
                    .map_err(|_| AppError::upstream("upload semaphore closed"))?;
                let part = upload_part(
                    &upload_client,
                    &bucket,
                    &object_key,
                    &upload_id,
                    part_number,
                    bytes,
                )
                .await?;

                Ok::<NumberedCompletedPart, AppError>(NumberedCompletedPart { part_number, part })
            }
        })
        .buffer_unordered(in_flight);

    while let Some(result) = tasks.next().await {
        match result {
            Ok(part) => completed_parts.push(part),
            Err(error) => {
                let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
                return Err(error);
            }
        }
    }
    drop(tasks);

    completed_parts.sort_by_key(|item| item.part_number);
    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(
            completed_parts.into_iter().map(|item| item.part).collect(),
        ))
        .build();

    let response = match complete_multipart_upload(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        completed,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    };

    Ok(UploadOutcome {
        object_key,
        etag: response.e_tag().map(ToOwned::to_owned),
        size_bytes: total_size as i64,
        parts_uploaded: total_parts,
        content_type: content_type.to_owned(),
        upload_mode: UploadMode::Multipart,
    })
}

async fn upload_multipart_from_unknown_ranges(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    first_part: Vec<u8>,
    _first_part_permit: OwnedSemaphorePermit,
    content_type: &str,
    chunk_size: usize,
) -> AppResult<UploadOutcome> {
    let object_key = build_object_key(job, content_type)?;
    let upload_id =
        create_multipart_upload(state, &runtime.s3_client, &object_key, content_type).await?;
    let bucket = state.settings.config.storage.bucket.clone();
    let mut parts = Vec::new();
    let mut total_bytes = first_part.len() as i64;

    match upload_part(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        1,
        first_part,
    )
    .await
    {
        Ok(part) => parts.push(part),
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    }
    drop(_first_part_permit);

    let mut next_start = chunk_size as u64;
    let mut part_number = 2_i32;
    loop {
        let end = next_start + chunk_size as u64 - 1;
        let _buffer_permit = acquire_buffer_permit(runtime).await?;
        let bytes = match download_range_bytes_optional(
            &state.transfer_http_client,
            &job.fetch_url,
            next_start,
            end,
        )
        .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(error) => {
                let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
                return Err(error);
            }
        };

        total_bytes += bytes.len() as i64;
        let is_last = bytes.len() < chunk_size;
        match upload_part(
            &runtime.s3_client,
            &bucket,
            &object_key,
            &upload_id,
            part_number,
            bytes,
        )
        .await
        {
            Ok(part) => parts.push(part),
            Err(error) => {
                let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
                return Err(error);
            }
        }

        if is_last {
            break;
        }
        next_start += chunk_size as u64;
        part_number += 1;
    }

    let parts_uploaded = parts.len() as i32;
    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(parts))
        .build();
    let response = match complete_multipart_upload(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        completed,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    };

    Ok(UploadOutcome {
        object_key,
        etag: response.e_tag().map(ToOwned::to_owned),
        size_bytes: total_bytes,
        parts_uploaded,
        content_type: content_type.to_owned(),
        upload_mode: UploadMode::Multipart,
    })
}

async fn upload_multipart_from_stream<S>(
    state: &AppState,
    runtime: &TransferRuntime,
    job: &TransferJob,
    first_part: Vec<u8>,
    _stream_buffer_permit: OwnedSemaphorePermit,
    remainder: Vec<u8>,
    mut stream: S,
    content_type: &str,
    chunk_size: usize,
) -> AppResult<UploadOutcome>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    let object_key = build_object_key(job, content_type)?;
    let upload_id =
        create_multipart_upload(state, &runtime.s3_client, &object_key, content_type).await?;
    let bucket = state.settings.config.storage.bucket.clone();
    let mut parts = Vec::new();
    let mut total_bytes = first_part.len() as i64;

    match upload_part(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        1,
        first_part,
    )
    .await
    {
        Ok(part) => parts.push(part),
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    }

    let mut buffer = remainder;
    let mut part_number = 2_i32;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| AppError::upstream(format!("source stream failed: {error}")))?;
        buffer.extend_from_slice(&chunk);

        while buffer.len() >= chunk_size {
            let tail = buffer.split_off(chunk_size);
            let current = std::mem::replace(&mut buffer, tail);
            total_bytes += current.len() as i64;
            match upload_part(
                &runtime.s3_client,
                &bucket,
                &object_key,
                &upload_id,
                part_number,
                current,
            )
            .await
            {
                Ok(part) => parts.push(part),
                Err(error) => {
                    let _ =
                        abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
                    return Err(error);
                }
            }
            part_number += 1;
        }
    }

    if !buffer.is_empty() {
        total_bytes += buffer.len() as i64;
        match upload_part(
            &runtime.s3_client,
            &bucket,
            &object_key,
            &upload_id,
            part_number,
            buffer,
        )
        .await
        {
            Ok(part) => parts.push(part),
            Err(error) => {
                let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
                return Err(error);
            }
        }
    }

    let parts_uploaded = parts.len() as i32;
    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(parts))
        .build();
    let response = match complete_multipart_upload(
        &runtime.s3_client,
        &bucket,
        &object_key,
        &upload_id,
        completed,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            let _ = abort_upload(&runtime.s3_client, &bucket, &object_key, &upload_id).await;
            return Err(error);
        }
    };

    Ok(UploadOutcome {
        object_key,
        etag: response.e_tag().map(ToOwned::to_owned),
        size_bytes: total_bytes,
        parts_uploaded,
        content_type: content_type.to_owned(),
        upload_mode: UploadMode::Multipart,
    })
}

async fn create_multipart_upload(
    state: &AppState,
    client: &Client,
    object_key: &str,
    content_type: &str,
) -> AppResult<String> {
    let create = client
        .create_multipart_upload()
        .bucket(&state.settings.config.storage.bucket)
        .key(object_key)
        .content_type(content_type)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to create multipart upload: {error}"))
        })?;

    create
        .upload_id()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::upstream("multipart upload id missing"))
}

async fn complete_multipart_upload(
    client: &Client,
    bucket: &str,
    object_key: &str,
    upload_id: &str,
    completed: CompletedMultipartUpload,
) -> AppResult<aws_sdk_s3::operation::complete_multipart_upload::CompleteMultipartUploadOutput> {
    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(object_key)
        .upload_id(upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to complete multipart upload: {error}"))
        })
}

async fn upload_part(
    client: &Client,
    bucket: &str,
    object_key: &str,
    upload_id: &str,
    part_number: i32,
    bytes: Vec<u8>,
) -> AppResult<CompletedPart> {
    let response = client
        .upload_part()
        .bucket(bucket)
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
    bucket: &str,
    object_key: &str,
    upload_id: &str,
) -> AppResult<()> {
    client
        .abort_multipart_upload()
        .bucket(bucket)
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
    media_id: Uuid,
    storage_object_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO media_storage_bindings (
            id, media_id, storage_object_id, object_role
        )
        VALUES ($1, $2, $3, 'original')
        ON CONFLICT (media_id, object_role) DO UPDATE
        SET storage_object_id = EXCLUDED.storage_object_id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(media_id)
    .bind(storage_object_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn download_range_bytes(
    client: &HttpClient,
    url: &str,
    start: u64,
    end: u64,
) -> AppResult<Vec<u8>> {
    let response = client
        .get(url)
        .header(RANGE, range_header_value(start, end))
        .send()
        .await?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::upstream(format!(
            "range download failed with status {}",
            response.status()
        )));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| AppError::upstream(format!("failed to read range body: {error}")))
}

async fn download_range_bytes_optional(
    client: &HttpClient,
    url: &str,
    start: u64,
    end: u64,
) -> AppResult<Option<Vec<u8>>> {
    let response = client
        .get(url)
        .header(RANGE, range_header_value(start, end))
        .send()
        .await?;

    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(None);
    }
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::upstream(format!(
            "range download failed with status {}",
            response.status()
        )));
    }

    response
        .bytes()
        .await
        .map(|bytes| Some(bytes.to_vec()))
        .map_err(|error| AppError::upstream(format!("failed to read range body: {error}")))
}

fn buffer_permit_count(state: &AppState) -> usize {
    buffer_permit_count_for(
        state.settings.config.transfer.memory_budget_bytes,
        state.settings.config.transfer.chunk_size_bytes,
    )
}

fn buffer_permit_count_for(memory_budget_bytes: usize, chunk_size_bytes: usize) -> usize {
    let chunk_size = chunk_size_bytes.max(1);
    let permits = memory_budget_bytes / chunk_size;
    permits.max(1)
}

fn lease_duration(state: &AppState) -> time::Duration {
    time::Duration::seconds(
        i64::try_from(state.settings.config.transfer.attempt_timeout_seconds).unwrap_or(i64::MAX),
    )
}

async fn acquire_buffer_permit(runtime: &TransferRuntime) -> AppResult<OwnedSemaphorePermit> {
    runtime
        .buffer_permits
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| AppError::upstream("buffer semaphore closed"))
}

fn build_object_key(job: &TransferJob, content_type: &str) -> AppResult<String> {
    let now = OffsetDateTime::now_utc();
    let digest = hex_digest(&job.fetch_url);
    let ext = extension_for_object(&job.fetch_url, content_type);
    Ok(format!(
        "{}/{}/{:02}/{:02}/{}/{}.{}",
        job.source_kind,
        now.year(),
        u8::from(now.month()),
        now.day(),
        sanitize_key_segment(&job.media_id.to_string()),
        digest,
        ext
    ))
}

fn extension_for_object(source_url: &str, content_type: &str) -> String {
    if let Ok(url) = Url::parse(source_url) {
        if let Some(format) = url
            .query_pairs()
            .find_map(|(key, value)| (key == "format").then(|| value.into_owned()))
        {
            if !format.trim().is_empty() {
                return format.to_ascii_lowercase();
            }
        }

        if let Some(last) = url.path_segments().and_then(|segments| segments.last()) {
            if let Some((_, ext)) = last.rsplit_once('.') {
                if !ext.is_empty() {
                    return ext.to_ascii_lowercase();
                }
            }
        }
    }

    match content_type {
        "video/mp4" => "mp4".to_owned(),
        "image/gif" => "gif".to_owned(),
        "image/png" => "png".to_owned(),
        "image/webp" => "webp".to_owned(),
        "image/jpeg" => "jpg".to_owned(),
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

fn range_header_value(start: u64, end: u64) -> String {
    format!("bytes={start}-{end}")
}

fn parse_total_size_from_content_range(response: &Response) -> Option<u64> {
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let (_, total) = value.rsplit_once('/')?;
    if total == "*" {
        return None;
    }
    total.parse().ok()
}

fn response_content_type(response: &Response, hint: &str) -> String {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let hint = hint.trim();
            (!hint.is_empty()).then(|| hint.to_owned())
        })
        .unwrap_or_else(|| "application/octet-stream".to_owned())
}

#[derive(Debug)]
struct NumberedCompletedPart {
    part_number: i32,
    part: CompletedPart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_permit_count_has_floor_of_one() {
        assert_eq!(buffer_permit_count_for(0, 1024), 1);
        assert_eq!(buffer_permit_count_for(512, 1024), 1);
        assert_eq!(buffer_permit_count_for(4096, 1024), 4);
    }

    #[test]
    fn extension_for_object_prefers_query_format() {
        let ext = extension_for_object(
            "https://pbs.twimg.com/media/demo?format=png&name=orig",
            "image/jpeg",
        );
        assert_eq!(ext, "png");
    }
}
