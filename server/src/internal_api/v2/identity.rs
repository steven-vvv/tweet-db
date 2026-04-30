use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    db,
    error::{AppError, AppResult},
    state::AppState,
};

use super::common::*;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserListQuery {
    #[serde(flatten)]
    list: ListQuery,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchUserRequest {
    disabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserCursor {
    v: u8,
    q: Option<String>,
    status: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserChildCursor {
    v: u8,
    user_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::IdentityRead)?;
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
        SELECT id, username::text AS username, is_admin, disabled_at, disabled_by_user_id, created_at, updated_at
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

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("id");
        (
            user_json_from_row(&row),
            UserCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::IdentityRead)?;
    let row = fetch_user_row(&state.db, user_id).await?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();

    if includes.contains("sessions") {
        included.insert(
            "sessions".to_owned(),
            fetch_user_sessions_array(&state.db, user_id, 20).await?,
        );
    }
    if includes.contains("sso-authorizations") {
        included.insert(
            "ssoAuthorizations".to_owned(),
            fetch_user_sso_authorizations_array(&state.db, user_id, 20).await?,
        );
    }
    if includes.contains("audit-events") {
        included.insert(
            "auditEvents".to_owned(),
            fetch_user_audit_events_array(&state.db, user_id, 20).await?,
        );
    }

    Ok(Json(detail_response(user_json_from_row(&row), included)))
}

pub async fn patch_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<PatchUserRequest>,
) -> AppResult<Json<ActionResponse>> {
    let admin = require_capability(session, Capability::IdentityWrite)?;
    let disabled = payload
        .disabled
        .ok_or_else(|| AppError::bad_request("disabled is required"))?;
    let user = if disabled {
        disable_user(&state, &admin, user_id).await?
    } else {
        enable_user(&state, &admin, user_id).await?
    };

    Ok(Json(action_response(
        user,
        json!({
            "ok": true,
            "transition": if disabled { "disable" } else { "enable" },
        }),
    )))
}

pub async fn list_user_sessions(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::IdentityRead)?;
    ensure_user_exists(&state.db, user_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<UserChildCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.user_id != user_id {
            return Err(AppError::bad_request("cursor does not match user"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT selector, user_id, sso_subject_id, authorization_id, registration_state, expires_at, last_seen_at, created_at
        FROM iam.sessions
        WHERE user_id = $1
          AND (
                NOT $2
            OR (created_at, selector) < ($3, $4)
          )
        ORDER BY created_at DESC, selector DESC
        LIMIT $5
        "#,
    )
    .bind(user_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("selector");
        (
            session_json_from_row(&row),
            UserChildCursor {
                v: CURSOR_VERSION,
                user_id,
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_user_sso_authorizations(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::IdentityRead)?;
    ensure_user_exists(&state.db, user_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<UserChildCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.user_id != user_id {
            return Err(AppError::bad_request("cursor does not match user"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT authorization_id, sso_subject_id, user_id, status, last_checked_at, remote_expires_at, revoked_at, created_at, updated_at
        FROM iam.user_sso_authorizations
        WHERE user_id = $1
          AND (
                NOT $2
            OR (created_at, authorization_id) < ($3, $4)
          )
        ORDER BY created_at DESC, authorization_id DESC
        LIMIT $5
        "#,
    )
    .bind(user_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("authorization_id");
        (
            sso_authorization_json_from_row(&row),
            UserChildCursor {
                v: CURSOR_VERSION,
                user_id,
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

async fn disable_user(state: &AppState, admin: &ActiveSession, user_id: Uuid) -> AppResult<Value> {
    if admin.record.user_id == Some(user_id) {
        return Err(AppError::bad_request(
            "cannot disable current admin account",
        ));
    }

    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
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
    let row = row.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    if disabled_at.is_none() {
        let now = OffsetDateTime::now_utc();
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
                "disabledAt": format_time(now),
            }),
        )
        .await?;
    }

    let user = fetch_user_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(user)
}

async fn enable_user(state: &AppState, admin: &ActiveSession, user_id: Uuid) -> AppResult<Value> {
    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
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
    let row = row.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

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

    let user = fetch_user_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(user)
}

async fn fetch_user_row(pool: &sqlx::PgPool, user_id: Uuid) -> AppResult<sqlx::postgres::PgRow> {
    let row = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, disabled_by_user_id, created_at, updated_at
        FROM iam.users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| AppError::not_found("user not found"))
}

async fn fetch_user_json_tx(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> AppResult<Value> {
    let row = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, disabled_by_user_id, created_at, updated_at
        FROM iam.users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(user_json_from_row(&row))
}

async fn ensure_user_exists(pool: &sqlx::PgPool, user_id: Uuid) -> AppResult<()> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM iam.users
            WHERE id = $1
        )
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    if exists {
        Ok(())
    } else {
        Err(AppError::not_found("user not found"))
    }
}

async fn fetch_user_sessions_array(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    limit: i64,
) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT selector, user_id, sso_subject_id, authorization_id, registration_state, expires_at, last_seen_at, created_at
        FROM iam.sessions
        WHERE user_id = $1
        ORDER BY created_at DESC, selector DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| session_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_user_sso_authorizations_array(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    limit: i64,
) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT authorization_id, sso_subject_id, user_id, status, last_checked_at, remote_expires_at, revoked_at, created_at, updated_at
        FROM iam.user_sso_authorizations
        WHERE user_id = $1
        ORDER BY created_at DESC, authorization_id DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| sso_authorization_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_user_audit_events_array(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    limit: i64,
) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
        FROM audit.audit_events
        WHERE resource_type = 'user'
          AND resource_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT $2
        "#,
    )
    .bind(user_id.to_string())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "actorUserId": row.get::<Option<Uuid>, _>("actor_user_id"),
                    "eventType": row.get::<String, _>("event_type"),
                    "resourceType": row.get::<String, _>("resource_type"),
                    "resourceId": row.get::<Option<String>, _>("resource_id"),
                    "details": row.get::<Value, _>("details"),
                    "createdAt": row_time(&row, "created_at"),
                })
            })
            .collect(),
    ))
}

fn user_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");
    json!({
        "id": row.get::<Uuid, _>("id"),
        "username": row.get::<String, _>("username"),
        "isAdmin": row.get::<bool, _>("is_admin"),
        "disabled": disabled_at.is_some(),
        "disabledAt": format_time_opt(disabled_at),
        "disabledByUserId": row.get::<Option<Uuid>, _>("disabled_by_user_id"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}

fn session_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("selector"),
        "userId": row.get::<Option<Uuid>, _>("user_id"),
        "ssoSubjectId": row.get::<Uuid, _>("sso_subject_id"),
        "authorizationId": row.get::<Uuid, _>("authorization_id"),
        "registrationState": row.get::<String, _>("registration_state"),
        "expiresAt": row_time(row, "expires_at"),
        "lastSeenAt": row_time(row, "last_seen_at"),
        "createdAt": row_time(row, "created_at"),
    })
}

fn sso_authorization_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("authorization_id"),
        "ssoSubjectId": row.get::<Uuid, _>("sso_subject_id"),
        "userId": row.get::<Option<Uuid>, _>("user_id"),
        "status": row.get::<String, _>("status"),
        "lastCheckedAt": row_time(row, "last_checked_at"),
        "remoteExpiresAt": row_time_opt(row, "remote_expires_at"),
        "revokedAt": row_time_opt(row, "revoked_at"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}
