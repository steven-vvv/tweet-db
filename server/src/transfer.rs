use std::{collections::HashMap, sync::Arc, time::Duration};

use aws_sdk_s3::Client as S3Client;
use bytes::Bytes;
use reqwest::{Client as HttpClient, header::CONTENT_TYPE};
use serde::Serialize;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use tokio::time::sleep;
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
    HttpClient::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(transfer.connect_timeout_seconds))
        .read_timeout(Duration::from_secs(transfer.read_timeout_seconds))
        .timeout(Duration::from_secs(transfer.attempt_timeout_seconds))
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
    let attempt_timeout =
        Duration::from_secs(runtime.settings.config.transfer.attempt_timeout_seconds);
    let max_attempts = runtime.settings.config.transfer.max_attempts;

    loop {
        let stale_cutoff = OffsetDateTime::now_utc() - duration_to_time(attempt_timeout);
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
    let downloaded = download_source(&runtime.download_client, &task.source_url).await?;
    let explicit_content_type = task
        .source_content_type
        .as_deref()
        .or(downloaded.content_type.as_deref());
    let uploaded = storage::upload_bytes(
        &runtime.settings,
        &runtime.storage_client,
        task.media_id,
        task.id,
        &task.source_url,
        explicit_content_type,
        downloaded.bytes,
    )
    .await?;
    persist_completed_task(db, task.id, uploaded).await
}

async fn download_source(client: &HttpClient, source_url: &str) -> AppResult<DownloadedSource> {
    let response = client.get(source_url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "download returned status {} for {}",
            response.status(),
            source_url
        )));
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response.bytes().await?;

    Ok(DownloadedSource {
        bytes,
        content_type,
    })
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

struct DownloadedSource {
    bytes: Bytes,
    content_type: Option<String>,
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
