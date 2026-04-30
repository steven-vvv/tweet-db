use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    error::{AppError, AppResult},
    state::AppState,
    storage,
};

use super::{
    common::*,
    media::transfer_task_select_sql,
    rows::{storage_object_json_from_row, transfer_task_json_from_row},
};

#[derive(Debug, Serialize, Deserialize)]
struct StorageObjectCursor {
    v: u8,
    q: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct StorageObjectTaskCursor {
    v: u8,
    object_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_storage_objects(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::StorageRead)?;
    let limit = resolve_limit(query.limit);
    let q = normalize_query(query.q.as_deref());
    let cursor = decode_cursor::<StorageObjectCursor>(query.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            object.id,
            object.provider,
            object.bucket,
            object.object_key,
            object.content_type,
            object.content_length,
            object.etag,
            object.sha256_hex,
            object.created_at,
            COALESCE(task_counts.task_count, 0) AS task_count
        FROM media.storage_object AS object
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS task_count
            FROM media.transfer_task AS task
            WHERE task.storage_object_id = object.id
        ) AS task_counts ON TRUE
        WHERE (
                $1::text IS NULL
            OR object.id::text LIKE $1
            OR object.object_key LIKE $1
        )
          AND (
                NOT $2
            OR (object.created_at, object.id) < ($3, $4)
          )
        ORDER BY object.created_at DESC, object.id DESC
        LIMIT $5
        "#,
    )
    .bind(q_prefix.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("id");
        (
            storage_object_json_from_row(&row),
            StorageObjectCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_storage_object(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::StorageRead)?;
    let row = fetch_storage_object_row(&state.db, object_id).await?;
    Ok(Json(detail_response(
        storage_object_json_from_row(&row),
        Default::default(),
    )))
}

pub async fn list_storage_object_transfer_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::StorageRead)?;
    fetch_storage_object_row(&state.db, object_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<StorageObjectTaskCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.object_id != object_id {
            return Err(AppError::bad_request(
                "cursor does not match storage object",
            ));
        }
    }

    let use_cursor = cursor.is_some();
    let sql = transfer_task_select_sql(
        r#"
        WHERE task.storage_object_id = $1
          AND (
                NOT $2
            OR (task.updated_at, task.id) < ($3, $4)
          )
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT $5
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(object_id)
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
            StorageObjectTaskCursor {
                v: CURSOR_VERSION,
                object_id,
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn create_storage_object_presigned_url(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
) -> AppResult<Json<ActionResponse>> {
    let _session = require_capability(session, Capability::StorageRead)?;
    let row = sqlx::query(
        r#"
        SELECT bucket, object_key
        FROM media.storage_object
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("storage object not found"))?;
    let expires_in = Duration::from_secs(300);
    let client = storage::build_client(&state.settings)?;
    let url = storage::presign_get_object(
        &client,
        &row.get::<String, _>("bucket"),
        &row.get::<String, _>("object_key"),
        expires_in,
    )
    .await?;
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(300);

    Ok(Json(action_response(
        json!({
            "id": object_id,
            "url": url,
            "expiresAt": format_time(expires_at),
        }),
        json!({ "ok": true }),
    )))
}

fn fetch_storage_object_sql() -> &'static str {
    r#"
    SELECT
        object.id,
        object.provider,
        object.bucket,
        object.object_key,
        object.content_type,
        object.content_length,
        object.etag,
        object.sha256_hex,
        object.created_at,
        COALESCE(task_counts.task_count, 0) AS task_count
    FROM media.storage_object AS object
    LEFT JOIN LATERAL (
        SELECT COUNT(*)::bigint AS task_count
        FROM media.transfer_task AS task
        WHERE task.storage_object_id = object.id
    ) AS task_counts ON TRUE
    WHERE object.id = $1
    "#
}

async fn fetch_storage_object_row(
    pool: &sqlx::PgPool,
    object_id: Uuid,
) -> AppResult<sqlx::postgres::PgRow> {
    sqlx::query(fetch_storage_object_sql())
        .bind(object_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("storage object not found"))
}
