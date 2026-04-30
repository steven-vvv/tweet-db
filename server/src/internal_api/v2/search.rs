use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    error::{AppError, AppResult},
    search::{self as search_domain, EnqueueStatus, IndexTarget},
    state::AppState,
};

use super::common::*;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexTaskListQuery {
    #[serde(flatten)]
    list: ListQuery,
    status: Option<String>,
    target_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueIndexTasksRequest {
    targets: Vec<EnqueueIndexTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueIndexTarget {
    target_kind: String,
    target_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexTaskCursor {
    v: u8,
    q: Option<String>,
    status: String,
    target_kind: String,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_index_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<IndexTaskListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::SearchRead)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let status = normalize_index_status(query.status.as_deref())?;
    let target_kind = normalize_index_target_kind(query.target_kind.as_deref())?;
    let cursor = decode_cursor::<IndexTaskCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        if cursor.status != status || cursor.target_kind != target_kind {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT id, target_kind::text AS target_kind, target_id, status::text AS status,
               attempt_count, last_error, claimed_by, claimed_at, completed_at, created_at, updated_at
        FROM search.index_queue
        WHERE ($1::text = 'all' OR status::text = $1)
          AND ($2::text = 'all' OR target_kind::text = $2)
          AND (
                $3::text IS NULL
            OR id::text LIKE $3
            OR target_id::text LIKE $3
          )
          AND (
                NOT $4
            OR (updated_at, id) < ($5, $6)
          )
        ORDER BY updated_at DESC, id DESC
        LIMIT $7
        "#,
    )
    .bind(&status)
    .bind(&target_kind)
    .bind(q_prefix.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.updated_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: Uuid = row.get("id");
        (
            index_task_json_from_row(&row),
            IndexTaskCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                target_kind: target_kind.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_index_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::SearchRead)?;
    let row = sqlx::query(
        r#"
        SELECT id, target_kind::text AS target_kind, target_id, status::text AS status,
               attempt_count, last_error, claimed_by, claimed_at, completed_at, created_at, updated_at
        FROM search.index_queue
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("search index task not found"))?;

    Ok(Json(detail_response(
        index_task_json_from_row(&row),
        Default::default(),
    )))
}

pub async fn enqueue_index_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<EnqueueIndexTasksRequest>,
) -> AppResult<Json<ActionResponse>> {
    let _session = require_capability(session, Capability::SearchWrite)?;
    let targets = payload
        .targets
        .iter()
        .map(parse_index_target)
        .collect::<AppResult<Vec<_>>>()?;
    let result = search_domain::enqueue_targets(&state.db, &targets).await?;
    let data = targets
        .into_iter()
        .map(|target| {
            let status = result
                .get(&target)
                .map(|status| match status {
                    EnqueueStatus::Enqueued => "enqueued",
                    EnqueueStatus::Refreshed => "refreshed",
                })
                .unwrap_or("skipped");
            json!({
                "targetKind": target.kind.as_db_str(),
                "targetId": json_i64(target.id),
                "status": status,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(action_response(
        Value::Array(data),
        json!({
            "ok": true,
            "count": result.len(),
        }),
    )))
}

fn parse_index_target(input: &EnqueueIndexTarget) -> AppResult<IndexTarget> {
    let id = input
        .target_id
        .trim()
        .parse::<i64>()
        .map_err(|_| AppError::bad_request("targetId must be a signed 64-bit integer"))?;
    match input.target_kind.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(IndexTarget::user(id)),
        "tweet" => Ok(IndexTarget::tweet(id)),
        _ => Err(AppError::bad_request(
            "targetKind must be one of user, tweet",
        )),
    }
}

fn normalize_index_status(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "pending" | "processing" | "completed" | "failed" => Ok(value),
        _ => Err(AppError::bad_request(
            "status must be one of all, pending, processing, completed, failed",
        )),
    }
}

fn index_task_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "targetKind": row.get::<String, _>("target_kind"),
        "targetId": json_i64(row.get::<i64, _>("target_id")),
        "status": row.get::<String, _>("status"),
        "attemptCount": row.get::<i32, _>("attempt_count"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "claimedBy": row.get::<Option<String>, _>("claimed_by"),
        "claimedAt": row_time_opt(row, "claimed_at"),
        "completedAt": row_time_opt(row, "completed_at"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}
