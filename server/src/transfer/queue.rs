use std::collections::HashMap;

use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    storage::StoredObjectMetadata,
};

use super::{EnqueueTransferTask, TransferEnqueueStatus};

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ClaimedTransferTask {
    pub(super) id: Uuid,
    pub(super) media_id: i64,
    pub(super) source_recorded_at: OffsetDateTime,
    pub(super) source_url: String,
    pub(super) source_kind: String,
    pub(super) source_content_type: Option<String>,
    pub(super) attempt_count: i32,
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

pub(super) async fn persist_completed_task(
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

pub(super) async fn expire_stale_tasks(
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

pub(super) async fn claim_next_task(
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

pub(super) async fn mark_task_failed(
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
