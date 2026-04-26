use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    future::Future,
    sync::Arc,
    time::Duration,
};

use aws_sdk_s3::Client as S3Client;
use bytes::{Bytes, BytesMut};
use futures_util::{
    StreamExt,
    future::{BoxFuture, FutureExt},
    stream::FuturesUnordered,
};
use reqwest::{
    Client as HttpClient, Response, StatusCode,
    header::{ACCEPT_RANGES, CONTENT_TYPE, RANGE},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use tokio::time::sleep;
use tokio::{sync::Semaphore, time::Instant};
use uuid::Uuid;

use crate::{
    config::{Settings, TransferSection},
    error::{AppError, AppResult},
    state::AppState,
    storage::{self, StoredObjectMetadata},
    tweet_model::{Media, MediaResource, MediaType},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSubsystemStatus {
    pub active: bool,
    pub worker_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedTransferSource {
    pub source_url: String,
    pub source_kind: &'static str,
    pub source_content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnqueueTransferTask {
    pub id: Uuid,
    pub media_id: i64,
    pub source_recorded_at: OffsetDateTime,
    pub source_url: String,
    pub source_kind: String,
    pub source_content_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferEnqueueStatus {
    Enqueued,
    AlreadyQueued,
    MissingSourceRecord,
}

#[derive(Clone)]
struct WorkerRuntime {
    settings: Arc<Settings>,
    download_client: HttpClient,
    storage_client: S3Client,
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimedTransferTask {
    id: Uuid,
    media_id: i64,
    source_recorded_at: OffsetDateTime,
    source_url: String,
    source_kind: String,
    source_content_type: Option<String>,
    attempt_count: i32,
}

pub fn status(section: &TransferSection) -> TransferSubsystemStatus {
    TransferSubsystemStatus {
        active: section.enabled,
        worker_count: section.worker_count,
    }
}

pub fn select_source(media: &Media, resource: &MediaResource) -> Option<SelectedTransferSource> {
    match media.media_type {
        MediaType::Photo => resource
            .media_url
            .as_ref()
            .map(|source_url| SelectedTransferSource {
                source_url: source_url.clone(),
                source_kind: "media_url",
                source_content_type: Some(storage::resolve_upload_content_type(
                    None,
                    source_url.as_str(),
                )),
            }),
        MediaType::Video | MediaType::AnimatedGif => {
            if let Some(video) = resource.video.as_ref() {
                if let Some(variant) = video
                    .variants
                    .iter()
                    .filter(|variant| variant.content_type.eq_ignore_ascii_case("video/mp4"))
                    .max_by_key(|variant| variant.bitrate.unwrap_or_default())
                    .or_else(|| video.variants.first())
                {
                    return Some(SelectedTransferSource {
                        source_url: variant.url.clone(),
                        source_kind: "video_variant",
                        source_content_type: Some(variant.content_type.clone()),
                    });
                }
            }

            resource
                .media_url
                .as_ref()
                .map(|source_url| SelectedTransferSource {
                    source_url: source_url.clone(),
                    source_kind: "media_url",
                    source_content_type: Some(storage::resolve_upload_content_type(
                        None,
                        source_url.as_str(),
                    )),
                })
        }
    }
}

pub async fn enqueue_tasks(
    db: &PgPool,
    tasks: &[EnqueueTransferTask],
) -> AppResult<HashMap<(i64, OffsetDateTime), TransferEnqueueStatus>> {
    if tasks.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        WITH input AS (
            SELECT DISTINCT ON (item.media_id, item.source_recorded_at)
                item.media_id,
                item.source_recorded_at,
                item.source_url,
                item.source_kind,
                item.source_content_type,
                item.id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id UUID,
                media_id BIGINT,
                source_recorded_at TIMESTAMPTZ,
                source_url TEXT,
                source_kind TEXT,
                source_content_type TEXT
            )
            ORDER BY item.media_id, item.source_recorded_at
        ),
        existing_source AS (
            SELECT input.media_id, input.source_recorded_at
            FROM input
            JOIN tweet.media_resource AS resource
              ON resource.media_id = input.media_id
             AND resource.recorded_at = input.source_recorded_at
        ),
        inserted AS (
            INSERT INTO media.transfer_task (
                id,
                media_id,
                source_recorded_at,
                source_url,
                source_kind,
                source_content_type,
                status
            )
            SELECT
                input.id,
                input.media_id,
                input.source_recorded_at,
                input.source_url,
                input.source_kind,
                input.source_content_type,
                'pending'::media.transfer_status
            FROM input
            JOIN existing_source
              ON existing_source.media_id = input.media_id
             AND existing_source.source_recorded_at = input.source_recorded_at
            ON CONFLICT (media_id, source_recorded_at) DO NOTHING
            RETURNING media_id, source_recorded_at
        )
        SELECT
            input.media_id,
            input.source_recorded_at,
            CASE
                WHEN existing_source.media_id IS NULL THEN 'missing_source'
                WHEN inserted.media_id IS NOT NULL THEN 'enqueued'
                ELSE 'duplicate'
            END AS status
        FROM input
        LEFT JOIN existing_source
          ON existing_source.media_id = input.media_id
         AND existing_source.source_recorded_at = input.source_recorded_at
        LEFT JOIN inserted
          ON inserted.media_id = input.media_id
         AND inserted.source_recorded_at = input.source_recorded_at
        "#,
    )
    .bind(serde_json::to_value(tasks)?)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|row| {
            let key = (
                row.get::<i64, _>("media_id"),
                row.get::<OffsetDateTime, _>("source_recorded_at"),
            );
            let status = match row.get::<String, _>("status").as_str() {
                "enqueued" => TransferEnqueueStatus::Enqueued,
                "duplicate" => TransferEnqueueStatus::AlreadyQueued,
                "missing_source" => TransferEnqueueStatus::MissingSourceRecord,
                other => {
                    return Err(AppError::upstream(format!(
                        "unexpected transfer enqueue status: {other}"
                    )));
                }
            };
            Ok((key, status))
        })
        .collect()
}

pub fn start_workers(state: AppState) -> AppResult<()> {
    let status = status(&state.settings.config.transfer);
    if !status.active {
        tracing::info!("media transfer workers are disabled by config");
        return Ok(());
    }

    if status.worker_count == 0 {
        tracing::info!("media transfer queue is enabled with zero workers");
        return Ok(());
    }

    let runtime = Arc::new(WorkerRuntime {
        settings: state.settings.clone(),
        download_client: build_download_client(&state.settings)?,
        storage_client: storage::build_client(&state.settings)?,
    });

    for worker_index in 0..status.worker_count {
        let runtime = runtime.clone();
        let db = state.db.clone();
        let worker_name = format!("transfer-worker-{}", worker_index + 1);
        tokio::spawn(async move {
            run_worker_loop(db, runtime, worker_name).await;
        });
    }

    tracing::info!(
        worker_count = status.worker_count,
        "started media transfer workers"
    );
    Ok(())
}

fn build_download_client(settings: &Settings) -> AppResult<HttpClient> {
    let transfer = &settings.config.transfer;
    let mut builder = HttpClient::builder().redirect(reqwest::redirect::Policy::none());
    if transfer.connect_timeout_seconds > 0 {
        builder = builder.connect_timeout(Duration::from_secs(transfer.connect_timeout_seconds));
    }
    if transfer.read_timeout_seconds > 0 {
        builder = builder.read_timeout(Duration::from_secs(transfer.read_timeout_seconds));
    }
    builder
        .build()
        .map_err(|error| AppError::config(format!("failed to build transfer http client: {error}")))
}

async fn run_worker_loop(db: PgPool, runtime: Arc<WorkerRuntime>, worker_name: String) {
    let poll_interval = Duration::from_secs(
        runtime
            .settings
            .config
            .transfer
            .worker_poll_interval_seconds,
    );
    let task_stale_timeout =
        Duration::from_secs(runtime.settings.config.transfer.task_stale_timeout_seconds);
    let max_attempts = runtime.settings.config.transfer.max_attempts;

    loop {
        let stale_cutoff = OffsetDateTime::now_utc() - duration_to_time(task_stale_timeout);
        if let Err(error) = expire_stale_tasks(&db, stale_cutoff, max_attempts).await {
            tracing::warn!(worker = %worker_name, error = %error, "failed to expire stale media transfer tasks");
        }

        match claim_next_task(&db, stale_cutoff, max_attempts, &worker_name).await {
            Ok(Some(task)) => {
                tracing::info!(
                    worker = %worker_name,
                    task_id = %task.id,
                    media_id = task.media_id,
                    source_kind = %task.source_kind,
                    source_recorded_at = %task.source_recorded_at,
                    attempt_count = task.attempt_count,
                    "processing media transfer task"
                );

                if let Err(error) = process_task(&db, &runtime, &task).await {
                    let message = truncate_error_message(&error.to_string(), 2048);
                    tracing::warn!(
                        worker = %worker_name,
                        task_id = %task.id,
                        media_id = task.media_id,
                        error = %message,
                        "media transfer task failed"
                    );
                    if let Err(update_error) =
                        mark_task_failed(&db, task.id, &message, max_attempts).await
                    {
                        tracing::warn!(
                            worker = %worker_name,
                            task_id = %task.id,
                            error = %update_error,
                            "failed to update media transfer task status after error"
                        );
                    }
                }
            }
            Ok(None) => sleep(poll_interval).await,
            Err(error) => {
                tracing::warn!(worker = %worker_name, error = %error, "failed to claim media transfer task");
                sleep(poll_interval).await;
            }
        }
    }
}

async fn process_task(
    db: &PgPool,
    runtime: &WorkerRuntime,
    task: &ClaimedTransferTask,
) -> AppResult<()> {
    let uploaded = transfer_source_to_storage(
        &runtime.settings,
        &runtime.download_client,
        &runtime.storage_client,
        task.media_id,
        task.id,
        &task.source_url,
        task.source_content_type.as_deref(),
    )
    .await?;
    persist_completed_task(db, task.id, uploaded).await
}

async fn transfer_source_to_storage(
    settings: &Settings,
    download_client: &HttpClient,
    storage_client: &S3Client,
    media_id: i64,
    task_id: Uuid,
    source_url: &str,
    source_content_type: Option<&str>,
) -> AppResult<StoredObjectMetadata> {
    let options = TransferOptions::from_section(&settings.config.transfer)?;
    let initial = open_initial_download(download_client, source_url, options.deadline).await?;
    let explicit_content_type = source_content_type.or(initial.content_type.as_deref());
    let object = storage::prepare_upload(
        settings,
        media_id,
        task_id,
        source_url,
        explicit_content_type,
    );
    let mut reader = ResponseByteReader::new(initial.response);

    let Some(first_buffer) =
        read_next_buffer(&mut reader, options.chunk_size_bytes, options.deadline).await?
    else {
        return put_single_object(storage_client, object, Bytes::new(), options.deadline).await;
    };

    if initial
        .content_length
        .is_some_and(|content_length| content_length <= first_buffer.len() as u64)
        || first_buffer.len() < options.chunk_size_bytes
    {
        return put_single_object(storage_client, object, first_buffer, options.deadline).await;
    }

    if initial.supports_ranges
        && options.download_parallelism > 1
        && let Some(content_length) = initial.content_length
    {
        drop(reader);
        return upload_multipart_range(
            download_client,
            storage_client,
            object,
            source_url,
            first_buffer,
            content_length,
            options,
        )
        .await;
    }

    let mut preloaded_buffers = Vec::new();
    if initial.content_length.is_none() {
        match read_next_buffer(&mut reader, options.chunk_size_bytes, options.deadline).await? {
            Some(second_buffer) => preloaded_buffers.push(second_buffer),
            None => {
                return put_single_object(storage_client, object, first_buffer, options.deadline)
                    .await;
            }
        }
    }

    upload_multipart_sequential(
        storage_client,
        object,
        first_buffer,
        preloaded_buffers,
        reader,
        options,
    )
    .await
}

async fn open_initial_download(
    client: &HttpClient,
    source_url: &str,
    deadline: Option<Instant>,
) -> AppResult<InitialDownload> {
    let response = with_deadline(deadline, "download request", async {
        client.get(source_url).send().await.map_err(AppError::from)
    })
    .await?;
    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "download returned status {} for {}",
            response.status(),
            source_url
        )));
    }

    let content_length = response.content_length();
    let supports_ranges = response_supports_ranges(&response);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    Ok(InitialDownload {
        response,
        content_type,
        content_length,
        supports_ranges,
    })
}

async fn put_single_object(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    body: Bytes,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    let sha256_hex = format!("{:x}", Sha256::digest(&body));
    with_deadline(
        deadline,
        "single object upload",
        storage::put_object(storage_client, object, body, sha256_hex),
    )
    .await
}

async fn upload_multipart_sequential(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    first_buffer: Bytes,
    preloaded_buffers: Vec<Bytes>,
    mut reader: ResponseByteReader,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload = with_deadline(
        options.deadline,
        "multipart upload creation",
        storage::create_multipart_upload(storage_client, &object),
    )
    .await?;
    let result = upload_multipart_sequential_inner(
        storage_client,
        object.clone(),
        upload.clone(),
        first_buffer,
        preloaded_buffers,
        &mut reader,
        options,
    )
    .await;

    finish_or_abort_multipart(storage_client, &object, &upload, result, options.deadline).await
}

async fn upload_multipart_sequential_inner(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    first_buffer: Bytes,
    preloaded_buffers: Vec<Bytes>,
    reader: &mut ResponseByteReader,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload_semaphore = Semaphore::new(options.upload_parallelism);
    let mut uploads: FuturesUnordered<UploadFuture<'_>> = FuturesUnordered::new();
    let mut completed_parts = Vec::new();
    let mut hasher = Sha256::new();
    let mut content_length = 0_u64;
    let mut next_part_number = 1_i32;

    enqueue_sequential_part(
        storage_client,
        &object,
        &upload,
        &upload_semaphore,
        options,
        &mut uploads,
        &mut hasher,
        &mut content_length,
        &mut next_part_number,
        first_buffer,
    )?;

    for buffer in preloaded_buffers {
        wait_for_sequential_capacity(&mut uploads, options, &mut completed_parts).await?;
        enqueue_sequential_part(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options,
            &mut uploads,
            &mut hasher,
            &mut content_length,
            &mut next_part_number,
            buffer,
        )?;
    }

    while let Some(buffer) =
        read_next_buffer(reader, options.chunk_size_bytes, options.deadline).await?
    {
        wait_for_sequential_capacity(&mut uploads, options, &mut completed_parts).await?;
        enqueue_sequential_part(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options,
            &mut uploads,
            &mut hasher,
            &mut content_length,
            &mut next_part_number,
            buffer,
        )?;
    }

    while let Some(uploaded) = uploads.next().await {
        completed_parts.push(uploaded?);
    }
    drop(uploads);
    drop(upload_semaphore);

    complete_multipart_with_hash(
        storage_client,
        object,
        upload,
        completed_parts,
        content_length,
        hasher,
        options.deadline,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn enqueue_sequential_part<'a>(
    storage_client: &'a S3Client,
    object: &'a storage::PreparedStorageObject,
    upload: &'a storage::MultipartUpload,
    upload_semaphore: &'a Semaphore,
    options: TransferOptions,
    uploads: &mut FuturesUnordered<UploadFuture<'a>>,
    hasher: &mut Sha256,
    content_length: &mut u64,
    next_part_number: &mut i32,
    buffer: Bytes,
) -> AppResult<()> {
    ensure_valid_part_number(*next_part_number)?;
    hasher.update(buffer.as_ref());
    add_content_length(content_length, buffer.len())?;
    uploads.push(
        upload_part_with_limit(
            storage_client,
            object,
            upload,
            upload_semaphore,
            options.deadline,
            *next_part_number,
            buffer,
        )
        .boxed(),
    );
    *next_part_number += 1;
    Ok(())
}

async fn wait_for_sequential_capacity<'a>(
    uploads: &mut FuturesUnordered<UploadFuture<'a>>,
    options: TransferOptions,
    completed_parts: &mut Vec<storage::UploadedPart>,
) -> AppResult<()> {
    while uploads.len() >= options.max_in_flight_parts {
        let Some(uploaded) = uploads.next().await else {
            break;
        };
        completed_parts.push(uploaded?);
    }
    Ok(())
}

async fn upload_multipart_range(
    download_client: &HttpClient,
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    source_url: &str,
    first_buffer: Bytes,
    content_length: u64,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload = with_deadline(
        options.deadline,
        "multipart upload creation",
        storage::create_multipart_upload(storage_client, &object),
    )
    .await?;
    let result = upload_multipart_range_inner(
        download_client,
        storage_client,
        object.clone(),
        upload.clone(),
        source_url,
        first_buffer,
        content_length,
        options,
    )
    .await;

    finish_or_abort_multipart(storage_client, &object, &upload, result, options.deadline).await
}

#[allow(clippy::too_many_arguments)]
async fn upload_multipart_range_inner(
    download_client: &HttpClient,
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    source_url: &str,
    first_buffer: Bytes,
    content_length: u64,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload_semaphore = Semaphore::new(options.upload_parallelism);
    let mut downloads: FuturesUnordered<DownloadFuture<'_>> = FuturesUnordered::new();
    let mut uploads: FuturesUnordered<UploadFuture<'_>> = FuturesUnordered::new();
    let mut pending_ranges = build_range_specs(
        first_buffer.len() as u64,
        content_length,
        options.chunk_size_bytes,
    )?;
    let mut part_states = BTreeMap::<i32, ActivePartState>::new();
    let mut completed_parts = Vec::new();
    let mut hasher = Sha256::new();
    let mut hashed_content_length = 0_u64;
    let mut next_hash_part = 1_i32;

    part_states.insert(1, ActivePartState::uploading_hashed());
    uploads.push(
        upload_part_with_limit(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options.deadline,
            1,
            first_buffer.clone(),
        )
        .boxed(),
    );
    part_states
        .get_mut(&1)
        .expect("first part state exists")
        .bytes = Some(first_buffer);
    hash_ready_range_parts(
        &mut part_states,
        &mut hasher,
        &mut hashed_content_length,
        &mut next_hash_part,
    )?;

    loop {
        while downloads.len() < options.download_parallelism
            && active_range_part_count(&downloads, &part_states) < options.max_in_flight_parts
            && let Some(spec) = pending_ranges.pop_front()
        {
            downloads.push(
                download_range_part(download_client, source_url, spec, options.deadline).boxed(),
            );
        }

        if pending_ranges.is_empty()
            && downloads.is_empty()
            && uploads.is_empty()
            && part_states.is_empty()
        {
            break;
        }

        tokio::select! {
            downloaded = downloads.next(), if !downloads.is_empty() => {
                let downloaded = downloaded
                    .expect("download future existed")?;
                part_states.insert(downloaded.part_number, ActivePartState::pending_hash(downloaded.bytes.clone()));
                uploads.push(upload_part_with_limit(
                    storage_client,
                    &object,
                    &upload,
                    &upload_semaphore,
                    options.deadline,
                    downloaded.part_number,
                    downloaded.bytes,
                ).boxed());
                hash_ready_range_parts(
                    &mut part_states,
                    &mut hasher,
                    &mut hashed_content_length,
                    &mut next_hash_part,
                )?;
            }
            uploaded = uploads.next(), if !uploads.is_empty() => {
                let uploaded = uploaded
                    .expect("upload future existed")?;
                mark_range_part_uploaded(&mut part_states, uploaded.part_number);
                completed_parts.push(uploaded);
            }
        }
    }

    if hashed_content_length != content_length {
        return Err(AppError::upstream(format!(
            "downloaded content length {} did not match expected content length {}",
            hashed_content_length, content_length
        )));
    }
    drop(downloads);
    drop(uploads);
    drop(upload_semaphore);

    complete_multipart_with_hash(
        storage_client,
        object,
        upload,
        completed_parts,
        hashed_content_length,
        hasher,
        options.deadline,
    )
    .await
}

async fn finish_or_abort_multipart(
    storage_client: &S3Client,
    object: &storage::PreparedStorageObject,
    upload: &storage::MultipartUpload,
    result: AppResult<StoredObjectMetadata>,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    match result {
        Ok(uploaded) => Ok(uploaded),
        Err(error) => {
            if let Err(abort_error) = with_deadline(
                deadline,
                "multipart upload abort",
                storage::abort_multipart_upload(storage_client, object, upload),
            )
            .await
            {
                tracing::warn!(error = %abort_error, "failed to abort multipart upload after transfer error");
            }
            Err(error)
        }
    }
}

async fn complete_multipart_with_hash(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    completed_parts: Vec<storage::UploadedPart>,
    content_length: u64,
    hasher: Sha256,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    let content_length = i64::try_from(content_length)
        .map_err(|_| AppError::upstream("object body exceeded i64 length limit"))?;
    let sha256_hex = format!("{:x}", hasher.finalize());

    with_deadline(
        deadline,
        "multipart upload completion",
        storage::complete_multipart_upload(
            storage_client,
            object,
            upload,
            completed_parts,
            content_length,
            sha256_hex,
        ),
    )
    .await
}

async fn upload_part_with_limit(
    storage_client: &S3Client,
    object: &storage::PreparedStorageObject,
    upload: &storage::MultipartUpload,
    upload_semaphore: &Semaphore,
    deadline: Option<Instant>,
    part_number: i32,
    buffer: Bytes,
) -> AppResult<storage::UploadedPart> {
    let _permit = upload_semaphore
        .acquire()
        .await
        .map_err(|_| AppError::upstream("multipart upload semaphore was closed"))?;
    with_deadline(
        deadline,
        "multipart part upload",
        storage::upload_multipart_part(storage_client, object, upload, part_number, buffer),
    )
    .await
}

async fn download_range_part(
    client: &HttpClient,
    source_url: &str,
    spec: RangeSpec,
    deadline: Option<Instant>,
) -> AppResult<DownloadedPart> {
    let header_value = format!("bytes={}-{}", spec.start, spec.end_inclusive);
    let response = with_deadline(deadline, "range download request", async {
        client
            .get(source_url)
            .header(RANGE, header_value.clone())
            .send()
            .await
            .map_err(AppError::from)
    })
    .await?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::upstream(format!(
            "range download returned status {} for {}",
            response.status(),
            source_url
        )));
    }

    if response
        .content_length()
        .is_some_and(|length| length != spec.len())
    {
        return Err(AppError::upstream(format!(
            "range download returned length {} for expected length {}",
            response.content_length().unwrap_or_default(),
            spec.len()
        )));
    }

    let expected_len = usize::try_from(spec.len())
        .map_err(|_| AppError::upstream("range part exceeded usize length limit"))?;
    let mut reader = ResponseByteReader::new(response);
    let mut buffer = BytesMut::with_capacity(expected_len);
    while buffer.len() < expected_len {
        let Some(bytes) = reader
            .read_some(expected_len - buffer.len(), deadline)
            .await?
        else {
            return Err(AppError::upstream(format!(
                "range download ended early for {}",
                source_url
            )));
        };
        buffer.extend_from_slice(&bytes);
    }

    Ok(DownloadedPart {
        part_number: spec.part_number,
        bytes: buffer.freeze(),
    })
}

#[derive(Debug, Clone, Copy)]
struct TransferOptions {
    chunk_size_bytes: usize,
    download_parallelism: usize,
    upload_parallelism: usize,
    max_in_flight_parts: usize,
    deadline: Option<Instant>,
}

impl TransferOptions {
    fn from_section(section: &TransferSection) -> AppResult<Self> {
        let chunk_size_bytes = section
            .chunk_size_mb
            .checked_mul(1024 * 1024)
            .ok_or_else(|| AppError::config("transfer.chunk_size_mb is too large"))?;
        let deadline = if section.attempt_timeout_seconds == 0 {
            None
        } else {
            Some(
                Instant::now()
                    .checked_add(Duration::from_secs(section.attempt_timeout_seconds))
                    .ok_or_else(|| {
                        AppError::config("transfer.attempt_timeout_seconds is too large")
                    })?,
            )
        };

        Ok(Self {
            chunk_size_bytes,
            download_parallelism: section.download_parallelism.max(1),
            upload_parallelism: section.upload_parallelism.max(1),
            max_in_flight_parts: section.max_in_flight_parts.max(1),
            deadline,
        })
    }
}

type UploadFuture<'a> = BoxFuture<'a, AppResult<storage::UploadedPart>>;
type DownloadFuture<'a> = BoxFuture<'a, AppResult<DownloadedPart>>;

struct InitialDownload {
    response: Response,
    content_type: Option<String>,
    content_length: Option<u64>,
    supports_ranges: bool,
}

struct ResponseByteReader {
    response: Response,
    pending: Option<Bytes>,
    pending_offset: usize,
}

impl ResponseByteReader {
    fn new(response: Response) -> Self {
        Self {
            response,
            pending: None,
            pending_offset: 0,
        }
    }

    async fn read_some(
        &mut self,
        max_len: usize,
        deadline: Option<Instant>,
    ) -> AppResult<Option<Bytes>> {
        loop {
            if let Some(pending) = self.pending.as_ref() {
                if self.pending_offset < pending.len() {
                    let end = (self.pending_offset + max_len).min(pending.len());
                    let bytes = pending.slice(self.pending_offset..end);
                    self.pending_offset = end;
                    if self.pending_offset == pending.len() {
                        self.pending = None;
                        self.pending_offset = 0;
                    }
                    return Ok(Some(bytes));
                }
                self.pending = None;
                self.pending_offset = 0;
            }

            let next = with_deadline(deadline, "download body read", async {
                self.response.chunk().await.map_err(AppError::from)
            })
            .await?;
            match next {
                Some(bytes) if bytes.is_empty() => {}
                Some(bytes) => {
                    self.pending = Some(bytes);
                    self.pending_offset = 0;
                }
                None => return Ok(None),
            }
        }
    }
}

async fn read_next_buffer(
    reader: &mut ResponseByteReader,
    chunk_size_bytes: usize,
    deadline: Option<Instant>,
) -> AppResult<Option<Bytes>> {
    let mut buffer = BytesMut::with_capacity(chunk_size_bytes);
    while buffer.len() < chunk_size_bytes {
        let Some(bytes) = reader
            .read_some(chunk_size_bytes - buffer.len(), deadline)
            .await?
        else {
            break;
        };
        buffer.extend_from_slice(&bytes);
    }

    if buffer.is_empty() {
        Ok(None)
    } else {
        Ok(Some(buffer.freeze()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RangeSpec {
    part_number: i32,
    start: u64,
    end_inclusive: u64,
}

impl RangeSpec {
    fn len(self) -> u64 {
        self.end_inclusive - self.start + 1
    }
}

#[derive(Debug)]
struct DownloadedPart {
    part_number: i32,
    bytes: Bytes,
}

#[derive(Debug)]
struct ActivePartState {
    bytes: Option<Bytes>,
    upload_done: bool,
}

impl ActivePartState {
    fn uploading_hashed() -> Self {
        Self {
            bytes: None,
            upload_done: false,
        }
    }

    fn pending_hash(bytes: Bytes) -> Self {
        Self {
            bytes: Some(bytes),
            upload_done: false,
        }
    }
}

fn build_range_specs(
    start_offset: u64,
    content_length: u64,
    chunk_size_bytes: usize,
) -> AppResult<VecDeque<RangeSpec>> {
    let chunk_size = u64::try_from(chunk_size_bytes)
        .map_err(|_| AppError::upstream("transfer chunk size exceeded u64 length limit"))?;
    let mut specs = VecDeque::new();
    let mut cursor = start_offset;
    let mut part_number = 2_i32;

    while cursor < content_length {
        ensure_valid_part_number(part_number)?;
        let next_cursor = cursor.saturating_add(chunk_size).min(content_length);
        specs.push_back(RangeSpec {
            part_number,
            start: cursor,
            end_inclusive: next_cursor - 1,
        });
        cursor = next_cursor;
        part_number += 1;
    }

    Ok(specs)
}

fn hash_ready_range_parts(
    part_states: &mut BTreeMap<i32, ActivePartState>,
    hasher: &mut Sha256,
    content_length: &mut u64,
    next_hash_part: &mut i32,
) -> AppResult<()> {
    loop {
        let part_number = *next_hash_part;
        let Some(state) = part_states.get_mut(&part_number) else {
            break;
        };
        let Some(bytes) = state.bytes.take() else {
            break;
        };
        hasher.update(bytes.as_ref());
        add_content_length(content_length, bytes.len())?;
        let remove_state = state.upload_done;
        *next_hash_part += 1;
        if remove_state {
            part_states.remove(&part_number);
        }
    }

    Ok(())
}

fn mark_range_part_uploaded(part_states: &mut BTreeMap<i32, ActivePartState>, part_number: i32) {
    let remove_state = if let Some(state) = part_states.get_mut(&part_number) {
        state.upload_done = true;
        state.bytes.is_none()
    } else {
        false
    };

    if remove_state {
        part_states.remove(&part_number);
    }
}

fn active_range_part_count<T>(
    downloads: &FuturesUnordered<T>,
    part_states: &BTreeMap<i32, ActivePartState>,
) -> usize {
    downloads.len() + part_states.len()
}

fn response_supports_ranges(response: &Response) -> bool {
    response
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("bytes"))
        })
}

fn ensure_valid_part_number(part_number: i32) -> AppResult<()> {
    if (1..=10_000).contains(&part_number) {
        Ok(())
    } else {
        Err(AppError::upstream(
            "multipart upload exceeded the S3 10000 part limit",
        ))
    }
}

fn add_content_length(total: &mut u64, len: usize) -> AppResult<()> {
    let len =
        u64::try_from(len).map_err(|_| AppError::upstream("buffer exceeded u64 length limit"))?;
    *total = total
        .checked_add(len)
        .ok_or_else(|| AppError::upstream("object body exceeded u64 length limit"))?;
    Ok(())
}

async fn with_deadline<T, F>(deadline: Option<Instant>, context: &str, future: F) -> AppResult<T>
where
    F: Future<Output = AppResult<T>>,
{
    if let Some(deadline) = deadline {
        tokio::time::timeout_at(deadline, future)
            .await
            .map_err(|_| AppError::upstream(format!("{context} timed out")))?
    } else {
        future.await
    }
}

async fn persist_completed_task(
    db: &PgPool,
    task_id: Uuid,
    uploaded: StoredObjectMetadata,
) -> AppResult<()> {
    let mut tx = db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO media.storage_object (
            id,
            provider,
            bucket,
            object_key,
            content_type,
            content_length,
            etag,
            sha256_hex
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(uploaded.id)
    .bind(&uploaded.provider)
    .bind(&uploaded.bucket)
    .bind(&uploaded.object_key)
    .bind(&uploaded.content_type)
    .bind(uploaded.content_length)
    .bind(uploaded.etag.as_deref())
    .bind(&uploaded.sha256_hex)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE media.transfer_task
        SET status = 'completed',
            storage_object_id = $2,
            last_error = NULL,
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .bind(uploaded.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn expire_stale_tasks(
    db: &PgPool,
    stale_cutoff: OffsetDateTime,
    max_attempts: i32,
) -> AppResult<u64> {
    let result = sqlx::query(
        r#"
        UPDATE media.transfer_task
        SET status = 'failed',
            last_error = COALESCE(last_error, 'attempt_timeout'),
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE status = 'processing'
          AND (claimed_at IS NULL OR claimed_at <= $1)
          AND attempt_count >= $2
        "#,
    )
    .bind(stale_cutoff)
    .bind(max_attempts)
    .execute(db)
    .await?;

    Ok(result.rows_affected())
}

async fn claim_next_task(
    db: &PgPool,
    stale_cutoff: OffsetDateTime,
    max_attempts: i32,
    worker_name: &str,
) -> AppResult<Option<ClaimedTransferTask>> {
    let row = sqlx::query_as::<_, ClaimedTransferTask>(
        r#"
        WITH candidate AS (
            SELECT id
            FROM media.transfer_task
            WHERE status = 'pending'
               OR (
                    status = 'processing'
                AND (claimed_at IS NULL OR claimed_at <= $1)
                AND attempt_count < $2
               )
            ORDER BY
                CASE WHEN status = 'pending' THEN 0 ELSE 1 END,
                created_at ASC,
                id ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE media.transfer_task AS task
        SET status = 'processing',
            attempt_count = task.attempt_count + 1,
            claimed_by = $3,
            claimed_at = NOW(),
            updated_at = NOW()
        FROM candidate
        WHERE task.id = candidate.id
        RETURNING
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.attempt_count
        "#,
    )
    .bind(stale_cutoff)
    .bind(max_attempts)
    .bind(worker_name)
    .fetch_optional(db)
    .await?;

    Ok(row)
}

async fn mark_task_failed(
    db: &PgPool,
    task_id: Uuid,
    error_message: &str,
    max_attempts: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE media.transfer_task
        SET status = CASE
                WHEN attempt_count >= $3 THEN 'failed'::media.transfer_status
                ELSE 'pending'::media.transfer_status
            END,
            last_error = $2,
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = CASE
                WHEN attempt_count >= $3 THEN NOW()
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .bind(error_message)
    .bind(max_attempts)
    .execute(db)
    .await?;

    Ok(())
}

fn duration_to_time(duration: Duration) -> time::Duration {
    time::Duration::seconds(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
}

fn truncate_error_message(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_photo_resource() -> MediaResource {
        MediaResource {
            media_id: 1,
            recorded_at: OffsetDateTime::parse(
                "2026-04-22T12:34:56Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap(),
            media_url: Some("https://pbs.twimg.com/media/demo.jpg".to_owned()),
            availability: Some("Available".to_owned()),
            video: None,
        }
    }

    #[test]
    fn transfer_subsystem_reports_configured_status() {
        let status = status(&TransferSection {
            enabled: true,
            worker_count: 2,
            chunk_size_mb: 10,
            download_parallelism: 4,
            upload_parallelism: 4,
            max_in_flight_parts: 8,
            connect_timeout_seconds: 5,
            read_timeout_seconds: 30,
            attempt_timeout_seconds: 300,
            task_stale_timeout_seconds: 900,
            worker_poll_interval_seconds: 5,
            max_attempts: 8,
        });

        assert!(status.active);
        assert_eq!(status.worker_count, 2);
    }

    #[test]
    fn photo_transfer_uses_media_url() {
        let source = select_source(
            &Media {
                id: 1,
                media_type: MediaType::Photo,
                ..Default::default()
            },
            &demo_photo_resource(),
        )
        .unwrap();

        assert_eq!(source.source_kind, "media_url");
        assert_eq!(source.source_url, "https://pbs.twimg.com/media/demo.jpg");
        assert_eq!(source.source_content_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn video_transfer_prefers_highest_bitrate_mp4_variant() {
        let source = select_source(
            &Media {
                id: 1,
                media_type: MediaType::Video,
                ..Default::default()
            },
            &MediaResource {
                video: Some(crate::tweet_model::MediaVideo {
                    aspect_ratio_w: Some(16),
                    aspect_ratio_h: Some(9),
                    duration_ms: Some(1200),
                    variants: vec![
                        crate::tweet_model::VideoVariant {
                            content_type: "application/x-mpegURL".to_owned(),
                            bitrate: None,
                            url: "https://video.twimg.com/demo.m3u8".to_owned(),
                        },
                        crate::tweet_model::VideoVariant {
                            content_type: "video/mp4".to_owned(),
                            bitrate: Some(832000),
                            url: "https://video.twimg.com/demo-832.mp4".to_owned(),
                        },
                        crate::tweet_model::VideoVariant {
                            content_type: "video/mp4".to_owned(),
                            bitrate: Some(2176000),
                            url: "https://video.twimg.com/demo-2176.mp4".to_owned(),
                        },
                    ],
                }),
                ..demo_photo_resource()
            },
        )
        .unwrap();

        assert_eq!(source.source_kind, "video_variant");
        assert_eq!(source.source_url, "https://video.twimg.com/demo-2176.mp4");
        assert_eq!(source.source_content_type.as_deref(), Some("video/mp4"));
    }
}
