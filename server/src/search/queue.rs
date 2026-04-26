use std::collections::HashMap;

use serde::Serialize;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    search::MAX_QUERY_CHARS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexTargetKind {
    User,
    Tweet,
}

impl IndexTargetKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Tweet => "tweet",
        }
    }

    fn from_db_str(value: &str) -> AppResult<Self> {
        match value {
            "user" => Ok(Self::User),
            "tweet" => Ok(Self::Tweet),
            other => Err(AppError::upstream(format!(
                "unexpected search index target kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexTarget {
    pub kind: IndexTargetKind,
    pub id: i64,
}

impl IndexTarget {
    pub fn user(id: i64) -> Self {
        Self {
            kind: IndexTargetKind::User,
            id,
        }
    }

    pub fn tweet(id: i64) -> Self {
        Self {
            kind: IndexTargetKind::Tweet,
            id,
        }
    }
}

#[derive(Debug, Serialize)]
struct EnqueuePayload {
    id: Uuid,
    target_kind: &'static str,
    target_id: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ClaimedIndexTask {
    pub(super) id: Uuid,
    pub(super) target_kind: String,
    pub(super) target_id: i64,
    pub(super) attempt_count: i32,
    pub(super) claimed_by: String,
    pub(super) claimed_at: OffsetDateTime,
}

impl ClaimedIndexTask {
    pub(super) fn parsed_kind(&self) -> AppResult<IndexTargetKind> {
        IndexTargetKind::from_db_str(&self.target_kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueStatus {
    Enqueued,
    Refreshed,
}

pub async fn enqueue_targets(
    db: &PgPool,
    targets: &[IndexTarget],
) -> AppResult<HashMap<IndexTarget, EnqueueStatus>> {
    if targets.is_empty() {
        return Ok(HashMap::new());
    }

    let payloads = targets
        .iter()
        .map(|target| EnqueuePayload {
            id: Uuid::now_v7(),
            target_kind: target.kind.as_db_str(),
            target_id: target.id,
        })
        .collect::<Vec<_>>();

    let rows = sqlx::query(
        r#"
        WITH input AS (
            SELECT DISTINCT ON (item.target_kind, item.target_id)
                item.id,
                item.target_kind::search.index_target_kind AS target_kind,
                item.target_id
            FROM jsonb_to_recordset($1::jsonb) AS item(
                id UUID,
                target_kind TEXT,
                target_id BIGINT
            )
            ORDER BY item.target_kind, item.target_id
        ),
        upserted AS (
            INSERT INTO search.index_queue (
                id,
                target_kind,
                target_id,
                status
            )
            SELECT
                input.id,
                input.target_kind,
                input.target_id,
                'pending'::search.index_task_status
            FROM input
            ON CONFLICT (target_kind, target_id) DO UPDATE
            SET status = 'pending',
                attempt_count = 0,
                last_error = NULL,
                claimed_by = NULL,
                claimed_at = NULL,
                completed_at = NULL,
                updated_at = NOW()
            RETURNING
                target_kind::text AS target_kind,
                target_id,
                (xmax = 0) AS inserted
        )
        SELECT target_kind, target_id, inserted
        FROM upserted
        "#,
    )
    .bind(serde_json::to_value(payloads)?)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|row| {
            let target = IndexTarget {
                kind: IndexTargetKind::from_db_str(row.get::<String, _>("target_kind").as_str())?,
                id: row.get("target_id"),
            };
            let status = if row.get::<bool, _>("inserted") {
                EnqueueStatus::Enqueued
            } else {
                EnqueueStatus::Refreshed
            };
            Ok((target, status))
        })
        .collect()
}

pub(super) async fn claim_next_tasks(
    db: &PgPool,
    stale_cutoff: OffsetDateTime,
    max_attempts: i32,
    worker_name: &str,
    batch_size: usize,
) -> AppResult<Vec<ClaimedIndexTask>> {
    let rows = sqlx::query_as::<_, ClaimedIndexTask>(
        r#"
        WITH candidate AS (
            SELECT id
            FROM search.index_queue
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
            LIMIT $4
        )
        UPDATE search.index_queue AS task
        SET status = 'processing',
            attempt_count = task.attempt_count + 1,
            claimed_by = $3,
            claimed_at = NOW(),
            updated_at = NOW()
        FROM candidate
        WHERE task.id = candidate.id
        RETURNING
            task.id,
            task.target_kind::text AS target_kind,
            task.target_id,
            task.attempt_count,
            task.claimed_by,
            task.claimed_at
        "#,
    )
    .bind(stale_cutoff)
    .bind(max_attempts)
    .bind(worker_name)
    .bind(i64::try_from(batch_size).unwrap_or(i64::MAX))
    .fetch_all(db)
    .await?;

    Ok(rows)
}

pub(super) async fn mark_task_completed(db: &PgPool, task: &ClaimedIndexTask) -> AppResult<bool> {
    let result = sqlx::query(
        r#"
        UPDATE search.index_queue
        SET status = 'completed',
            last_error = NULL,
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
          AND claimed_at = $3
        "#,
    )
    .bind(task.id)
    .bind(&task.claimed_by)
    .bind(task.claimed_at)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub(super) async fn mark_task_failed(
    db: &PgPool,
    task: &ClaimedIndexTask,
    error_message: &str,
    max_attempts: i32,
) -> AppResult<bool> {
    let truncated = truncate_error_message(error_message);
    let result = sqlx::query(
        r#"
        UPDATE search.index_queue
        SET status = CASE
                WHEN attempt_count >= $3 THEN 'failed'::search.index_task_status
                ELSE 'pending'::search.index_task_status
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
          AND status = 'processing'
          AND claimed_by = $4
          AND claimed_at = $5
        "#,
    )
    .bind(task.id)
    .bind(&truncated)
    .bind(max_attempts)
    .bind(&task.claimed_by)
    .bind(task.claimed_at)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub(super) async fn expire_stale_tasks(
    db: &PgPool,
    stale_cutoff: OffsetDateTime,
    max_attempts: i32,
) -> AppResult<u64> {
    let result = sqlx::query(
        r#"
        UPDATE search.index_queue
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

fn truncate_error_message(value: &str) -> String {
    value.chars().take(MAX_QUERY_CHARS * 8).collect()
}
