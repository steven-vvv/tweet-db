use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{auth::ActiveSession, error::AppResult, state::AppState};

use super::common::*;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventListQuery {
    #[serde(flatten)]
    list: ListQuery,
    actor_user_id: Option<Uuid>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    event_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditEventCursor {
    v: u8,
    actor_user_id: Option<Uuid>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    event_type: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_events(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<AuditEventListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::AuditRead)?;
    let limit = resolve_limit(query.list.limit);
    let resource_type = normalize_filter(query.resource_type.as_deref());
    let resource_id = normalize_filter(query.resource_id.as_deref());
    let event_type = normalize_filter(query.event_type.as_deref());
    let cursor = decode_cursor::<AuditEventCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.actor_user_id != query.actor_user_id
            || cursor.resource_type != resource_type
            || cursor.resource_id != resource_id
            || cursor.event_type != event_type
        {
            return Err(crate::error::AppError::bad_request(
                "cursor does not match filters",
            ));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
        FROM audit.audit_events
        WHERE ($1::uuid IS NULL OR actor_user_id = $1)
          AND ($2::text IS NULL OR resource_type = $2)
          AND ($3::text IS NULL OR resource_id = $3)
          AND ($4::text IS NULL OR event_type = $4)
          AND (
                NOT $5
            OR (created_at, id) < ($6, $7)
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $8
        "#,
    )
    .bind(query.actor_user_id)
    .bind(resource_type.as_deref())
    .bind(resource_id.as_deref())
    .bind(event_type.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let actor_user_id = query.actor_user_id;
    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("id");
        (
            audit_event_json_from_row(&row),
            AuditEventCursor {
                v: CURSOR_VERSION,
                actor_user_id,
                resource_type: resource_type.clone(),
                resource_id: resource_id.clone(),
                event_type: event_type.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_event(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(event_id): Path<Uuid>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::AuditRead)?;
    let row = sqlx::query(
        r#"
        SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
        FROM audit.audit_events
        WHERE id = $1
        "#,
    )
    .bind(event_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| crate::error::AppError::not_found("audit event not found"))?;

    Ok(Json(detail_response(
        audit_event_json_from_row(&row),
        Default::default(),
    )))
}

fn normalize_filter(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn audit_event_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "actorUserId": row.get::<Option<Uuid>, _>("actor_user_id"),
        "eventType": row.get::<String, _>("event_type"),
        "resourceType": row.get::<String, _>("resource_type"),
        "resourceId": row.get::<Option<String>, _>("resource_id"),
        "details": row.get::<Value, _>("details"),
        "createdAt": row_time(row, "created_at"),
    })
}
