use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    db,
    error::{AppError, AppResult},
    state::AppState,
};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const CURSOR_VERSION: u8 = 1;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserCursor {
    v: u8,
    q: Option<String>,
    status: String,
    #[serde(with = "cursor_datetime")]
    created_at: OffsetDateTime,
    id: Uuid,
}

mod cursor_datetime;

mod support;
#[cfg(test)]
mod tests;

use self::support::*;

pub async fn list_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let status = normalize_user_status(query.status.as_deref())?;
    let cursor = decode_cursor::<UserCursor>(query.list.cursor.as_deref())?;

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

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM iam.users
        WHERE (
                $1::text = 'all'
            OR ($1 = 'active' AND disabled_at IS NULL)
            OR ($1 = 'disabled' AND disabled_at IS NOT NULL)
        )
          AND (
                $2::text IS NULL
            OR username::text ILIKE $2
            OR id::text ILIKE $2
          )
          AND (
                NOT $3
            OR (created_at, id) < ($4, $5)
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $6
        "#,
    )
    .bind(&status)
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");
        let id: Uuid = row.get("id");
        (
            json!({
                "id": id,
                "username": row.get::<String, _>("username"),
                "isAdmin": row.get::<bool, _>("is_admin"),
                "disabled": disabled_at.is_some(),
                "disabledAt": format_time_opt(disabled_at),
                "createdAt": format_time(created_at),
                "updatedAt": format_time(row.get("updated_at")),
            }),
            UserCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            u.id,
            u.username::text AS username,
            u.is_admin,
            u.disabled_at,
            u.disabled_by_user_id,
            u.created_at,
            u.updated_at,
            to_jsonb(u) AS record,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT selector, user_id, sso_subject_id, authorization_id, registration_state, expires_at, last_seen_at, created_at
                    FROM iam.sessions
                    WHERE user_id = u.id
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS sessions,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT authorization_id, sso_subject_id, user_id, status, last_checked_at, remote_expires_at, revoked_at, created_at, updated_at
                    FROM iam.user_sso_authorizations
                    WHERE user_id = u.id
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS authorizations,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
                    FROM audit.audit_events
                    WHERE resource_type = 'user'
                      AND resource_id = u.id::text
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS audit_events
        FROM iam.users u
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    Ok(Json(detail_response(
        json!({
            "id": row.get::<Uuid, _>("id"),
            "username": row.get::<String, _>("username"),
            "isAdmin": row.get::<bool, _>("is_admin"),
            "disabled": disabled_at.is_some(),
            "disabledAt": format_time_opt(disabled_at),
            "disabledByUserId": row.get::<Option<Uuid>, _>("disabled_by_user_id"),
            "createdAt": format_time(row.get("created_at")),
            "updatedAt": format_time(row.get("updated_at")),
        }),
        row.get("record"),
        json!({
            "sessions": row.get::<Value, _>("sessions"),
            "authorizations": row.get::<Value, _>("authorizations"),
            "auditEvents": row.get::<Value, _>("audit_events"),
        }),
    )))
}

pub async fn disable_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    if admin.record.user_id == Some(user_id) {
        return Err(AppError::bad_request(
            "cannot disable current admin account",
        ));
    }

    let mut tx = state.db.begin().await?;
    let existing = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM iam.users
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = existing.ok_or_else(|| AppError::not_found("user not found"))?;
    let already_disabled: Option<OffsetDateTime> = existing.get("disabled_at");
    let now = OffsetDateTime::now_utc();

    if already_disabled.is_none() {
        sqlx::query(
            r#"
            UPDATE iam.users
            SET disabled_at = $2,
                disabled_by_user_id = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .bind(now)
        .bind(admin.record.user_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM iam.sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        db::insert_audit_event_tx(
            &mut tx,
            admin.record.user_id,
            "user.disabled",
            "user",
            Some(user_id.to_string()),
            json!({
                "disabled_at": format_time(now),
            }),
        )
        .await?;
    }

    let updated = fetch_user_summary_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "user": updated,
    })))
}

pub async fn enable_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    let mut tx = state.db.begin().await?;
    let existing = sqlx::query(
        r#"
        SELECT id, disabled_at
        FROM iam.users
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = existing.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = existing.get("disabled_at");

    if disabled_at.is_some() {
        sqlx::query(
            r#"
            UPDATE iam.users
            SET disabled_at = NULL,
                disabled_by_user_id = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        db::insert_audit_event_tx(
            &mut tx,
            admin.record.user_id,
            "user.enabled",
            "user",
            Some(user_id.to_string()),
            json!({}),
        )
        .await?;
    }

    let updated = fetch_user_summary_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "user": updated,
    })))
}

async fn fetch_user_summary_json_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> AppResult<Value> {
    let row = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM iam.users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    Ok(json!({
        "id": row.get::<Uuid, _>("id"),
        "username": row.get::<String, _>("username"),
        "isAdmin": row.get::<bool, _>("is_admin"),
        "disabled": disabled_at.is_some(),
        "disabledAt": format_time_opt(disabled_at),
        "createdAt": format_time(row.get("created_at")),
        "updatedAt": format_time(row.get("updated_at")),
    }))
}
