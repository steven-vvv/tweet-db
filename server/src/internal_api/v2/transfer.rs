use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    db,
    error::{AppError, AppResult},
    state::AppState,
};

use super::{common::*, media::transfer_task_select_sql, rows::transfer_task_json_from_row};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferTaskListQuery {
    #[serde(flatten)]
    list: ListQuery,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRequest {
    r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TransferTaskCursor {
    v: u8,
    q: Option<String>,
    status: String,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TransferTaskListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let status = normalize_transfer_status(query.status.as_deref())?;
    let cursor = decode_cursor::<TransferTaskCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        ensure_filter_match(
            Some(cursor.status.as_str()),
            Some(status.as_str()),
            "cursor does not match status filter",
        )?;
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let sql = transfer_task_select_sql(
        r#"
        WHERE (
                $1::text = 'all'
            OR task.status::text = $1
        )
          AND (
                $2::text IS NULL
            OR task.id::text LIKE $2
            OR task.media_id::text LIKE $2
            OR COALESCE(object.object_key, '') LIKE $2
          )
          AND (
                NOT $3
            OR (task.updated_at, task.id) < ($4, $5)
          )
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT $6
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(&status)
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
            transfer_task_json_from_row(&row),
            TransferTaskCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    let row = fetch_transfer_task_row(&state.db, task_id).await?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();
    if includes.contains("audit-events") {
        included.insert(
            "auditEvents".to_owned(),
            fetch_transfer_audit_events(&state.db, task_id, 20).await?,
        );
    }

    Ok(Json(detail_response(
        transfer_task_json_from_row(&row),
        included,
    )))
}

pub async fn transition_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<TransitionRequest>,
) -> AppResult<Json<ActionResponse>> {
    let admin = require_capability(session, Capability::TransferWrite)?;
    let task = match payload.r#type.as_str() {
        "retry" => retry_task(&state, &admin, task_id).await?,
        "cancel" => cancel_task(&state, &admin, task_id).await?,
        "release" => release_task(&state, &admin, task_id).await?,
        _ => {
            return Err(AppError::bad_request(
                "type must be one of retry, cancel, release",
            ));
        }
    };

    Ok(Json(action_response(
        task,
        json!({
            "ok": true,
            "transition": payload.r#type,
        }),
    )))
}

async fn retry_task(
    state: &AppState,
    admin: &ActiveSession,
    task_id: Uuid,
) -> AppResult<serde_json::Value> {
    transition_locked_task(
        state,
        admin,
        task_id,
        &["failed", "canceled"],
        "transfer.retried",
        r#"
        UPDATE media.transfer_task
        SET status = 'pending',
            last_error = NULL,
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .await
}

async fn cancel_task(
    state: &AppState,
    admin: &ActiveSession,
    task_id: Uuid,
) -> AppResult<serde_json::Value> {
    transition_locked_task(
        state,
        admin,
        task_id,
        &["pending"],
        "transfer.canceled",
        r#"
        UPDATE media.transfer_task
        SET status = 'canceled',
            last_error = 'canceled_by_admin',
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .await
}

async fn release_task(
    state: &AppState,
    admin: &ActiveSession,
    task_id: Uuid,
) -> AppResult<serde_json::Value> {
    transition_locked_task(
        state,
        admin,
        task_id,
        &["processing"],
        "transfer.released",
        r#"
        UPDATE media.transfer_task
        SET status = 'pending',
            last_error = 'released_by_admin',
            claimed_by = NULL,
            claimed_at = NULL,
            completed_at = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .await
}

async fn transition_locked_task(
    state: &AppState,
    admin: &ActiveSession,
    task_id: Uuid,
    allowed_statuses: &[&str],
    event_type: &str,
    update_sql: &str,
) -> AppResult<serde_json::Value> {
    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT id, media_id, status::text AS status
        FROM media.transfer_task
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(task_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("transfer task not found"))?;

    let status: String = row.get("status");
    if !allowed_statuses.iter().any(|allowed| *allowed == status) {
        return Err(AppError::bad_request(format!(
            "task status must be one of {}",
            allowed_statuses.join(", ")
        )));
    }

    sqlx::query(update_sql)
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

    db::insert_audit_event_tx(
        &mut tx,
        admin.record.user_id,
        event_type,
        "media_transfer_task",
        Some(task_id.to_string()),
        json!({
            "mediaId": json_i64(row.get::<i64, _>("media_id")),
            "previousStatus": status,
        }),
    )
    .await?;
    tx.commit().await?;

    let row = fetch_transfer_task_row(&state.db, task_id).await?;
    Ok(transfer_task_json_from_row(&row))
}

pub(super) async fn fetch_transfer_task_row(
    pool: &sqlx::PgPool,
    task_id: Uuid,
) -> AppResult<sqlx::postgres::PgRow> {
    let sql = transfer_task_select_sql("WHERE task.id = $1");
    sqlx::query(&sql)
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("transfer task not found"))
}

async fn fetch_transfer_audit_events(
    pool: &sqlx::PgPool,
    task_id: Uuid,
    limit: i64,
) -> AppResult<serde_json::Value> {
    let rows = sqlx::query(
        r#"
        SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
        FROM audit.audit_events
        WHERE resource_type = 'media_transfer_task'
          AND resource_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT $2
        "#,
    )
    .bind(task_id.to_string())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "actorUserId": row.get::<Option<Uuid>, _>("actor_user_id"),
                    "eventType": row.get::<String, _>("event_type"),
                    "resourceType": row.get::<String, _>("resource_type"),
                    "resourceId": row.get::<Option<String>, _>("resource_id"),
                    "details": row.get::<serde_json::Value, _>("details"),
                    "createdAt": row_time(&row, "created_at"),
                })
            })
            .collect(),
    ))
}
