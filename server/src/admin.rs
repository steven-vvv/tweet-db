use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    db,
    error::{AppError, AppResult},
    state::AppState,
    storage,
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

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceScopedListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub source_kind: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferJobListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserCursor {
    v: u8,
    q: Option<String>,
    status: String,
    created_at: OffsetDateTime,
    id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct PostCursor {
    v: u8,
    q: Option<String>,
    source_kind: Option<String>,
    last_observed_at: OffsetDateTime,
    row_source_kind: String,
    source_post_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActorCursor {
    v: u8,
    q: Option<String>,
    source_kind: Option<String>,
    last_observed_at: OffsetDateTime,
    row_source_kind: String,
    source_actor_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StorageObjectCursor {
    v: u8,
    q: Option<String>,
    created_at: OffsetDateTime,
    id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct TransferJobCursor {
    v: u8,
    q: Option<String>,
    status: Option<String>,
    updated_at: OffsetDateTime,
    id: Uuid,
}

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
        FROM users
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
            id,
            username::text AS username,
            is_admin,
            disabled_at,
            disabled_by_user_id,
            created_at,
            updated_at,
            to_jsonb(users) AS record
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    let sessions = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT selector, user_id, sso_subject_id, authorization_id, registration_state, expires_at, last_seen_at, created_at
            FROM sessions
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 20
        ) t
        "#,
        user_id,
    )
    .await?;
    let authorizations = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT authorization_id, sso_subject_id, user_id, status, last_checked_at, remote_expires_at, revoked_at, created_at, updated_at
            FROM user_sso_authorizations
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 20
        ) t
        "#,
        user_id,
    )
    .await?;
    let audit_events = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
            FROM audit_events
            WHERE resource_type = 'user'
              AND resource_id = $1
            ORDER BY created_at DESC
            LIMIT 20
        ) t
        "#,
        user_id.to_string(),
    )
    .await?;

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
            "sessions": sessions,
            "authorizations": authorizations,
            "auditEvents": audit_events,
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
        FROM users
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
            UPDATE users
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

        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
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
        FROM users
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
            UPDATE users
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

pub async fn list_posts(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<SourceScopedListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let source_kind = normalize_optional_source_kind(query.source_kind.as_deref())?;
    let cursor = decode_cursor::<PostCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        ensure_filter_match(
            cursor.source_kind.as_deref(),
            source_kind.as_deref(),
            "cursor does not match sourceKind filter",
        )?;
    }

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            p.source_kind,
            p.source_post_id,
            p.author_source_actor_id,
            p.full_text,
            p.lang,
            p.last_observed_at,
            COALESCE(pm.media_count, 0) AS media_count
        FROM posts p
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM post_media pm
            WHERE pm.source_kind = p.source_kind
              AND pm.source_post_id = p.source_post_id
        ) pm ON TRUE
        WHERE ($1::text IS NULL OR p.source_kind = $1)
          AND (
                $2::text IS NULL
            OR p.source_post_id ILIKE $2
            OR p.author_source_actor_id ILIKE $2
            OR p.full_text ILIKE $2
          )
          AND (
                NOT $3
            OR (p.last_observed_at, p.source_kind, p.source_post_id) < ($4, $5, $6)
          )
        ORDER BY p.last_observed_at DESC, p.source_kind DESC, p.source_post_id DESC
        LIMIT $7
        "#,
    )
    .bind(source_kind.as_deref())
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.last_observed_at))
    .bind(cursor.as_ref().map(|item| item.row_source_kind.as_str()))
    .bind(cursor.as_ref().map(|item| item.source_post_id.as_str()))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let last_observed_at: OffsetDateTime = row.get("last_observed_at");
        let row_source_kind: String = row.get("source_kind");
        let source_post_id: String = row.get("source_post_id");
        (
            json!({
                "sourceKind": row_source_kind,
                "sourcePostId": source_post_id,
                "authorSourceActorId": row.get::<String, _>("author_source_actor_id"),
                "fullText": row.get::<String, _>("full_text"),
                "lang": row.get::<String, _>("lang"),
                "mediaCount": row.get::<i64, _>("media_count"),
                "lastObservedAt": format_time(last_observed_at),
            }),
            PostCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                source_kind: source_kind.clone(),
                last_observed_at,
                row_source_kind: row.get("source_kind"),
                source_post_id: row.get("source_post_id"),
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_post(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path((source_kind, source_post_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let source_kind = normalize_source_kind(&source_kind)?;
    let row = sqlx::query(
        r#"
        SELECT
            p.source_kind,
            p.source_post_id,
            p.author_source_actor_id,
            p.full_text,
            p.last_observed_at,
            COALESCE(pm.media_count, 0) AS media_count,
            to_jsonb(p) AS record
        FROM posts p
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM post_media pm
            WHERE pm.source_kind = p.source_kind
              AND pm.source_post_id = p.source_post_id
        ) pm ON TRUE
        WHERE p.source_kind = $1
          AND p.source_post_id = $2
        "#,
    )
    .bind(&source_kind)
    .bind(source_post_id.trim())
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("post not found"))?;

    let metrics = fetch_optional_json2(
        &state.db,
        r#"
        SELECT to_jsonb(t)
        FROM (
            SELECT submission_id, source_kind, source_post_id, observed_at, view_count, favorite_count, retweet_count, reply_count, quote_count, bookmark_count, created_at
            FROM post_metric_observations
            WHERE source_kind = $1
              AND source_post_id = $2
            ORDER BY observed_at DESC, created_at DESC
            LIMIT 1
        ) t
        "#,
        source_kind.as_str(),
        source_post_id.trim(),
    )
    .await?;
    let author_source_actor_id = row.get::<String, _>("author_source_actor_id");
    let author = fetch_optional_json2(
        &state.db,
        r#"
        SELECT to_jsonb(t)
        FROM (
            SELECT
                a.source_kind,
                a.source_actor_id,
                a.last_observed_at,
                apv.name,
                apv.screen_name,
                apv.avatar_media_id,
                apv.banner_media_id
            FROM actors a
            LEFT JOIN actor_profile_versions apv ON apv.id = a.current_profile_version_id
            WHERE a.source_kind = $1
              AND a.source_actor_id = $2
            LIMIT 1
        ) t
        "#,
        source_kind.as_str(),
        author_source_actor_id.as_str(),
    )
    .await?;
    let media = fetch_post_media_json(&state.db, &source_kind, source_post_id.trim()).await?;

    Ok(Json(detail_response(
        json!({
            "sourceKind": row.get::<String, _>("source_kind"),
            "sourcePostId": row.get::<String, _>("source_post_id"),
            "authorSourceActorId": row.get::<String, _>("author_source_actor_id"),
            "mediaCount": row.get::<i64, _>("media_count"),
            "lastObservedAt": format_time(row.get("last_observed_at")),
        }),
        row.get("record"),
        json!({
            "latestMetrics": metrics,
            "author": author,
            "media": media,
        }),
    )))
}

pub async fn list_actors(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<SourceScopedListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let source_kind = normalize_optional_source_kind(query.source_kind.as_deref())?;
    let cursor = decode_cursor::<ActorCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        ensure_filter_match(
            cursor.source_kind.as_deref(),
            source_kind.as_deref(),
            "cursor does not match sourceKind filter",
        )?;
    }

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            a.source_kind,
            a.source_actor_id,
            a.last_observed_at,
            COALESCE(apv.name, '') AS name,
            COALESCE(apv.screen_name, '') AS screen_name
        FROM actors a
        LEFT JOIN actor_profile_versions apv ON apv.id = a.current_profile_version_id
        WHERE ($1::text IS NULL OR a.source_kind = $1)
          AND (
                $2::text IS NULL
            OR a.source_actor_id ILIKE $2
            OR COALESCE(apv.screen_name, '') ILIKE $2
            OR COALESCE(apv.name, '') ILIKE $2
          )
          AND (
                NOT $3
            OR (a.last_observed_at, a.source_kind, a.source_actor_id) < ($4, $5, $6)
          )
        ORDER BY a.last_observed_at DESC, a.source_kind DESC, a.source_actor_id DESC
        LIMIT $7
        "#,
    )
    .bind(source_kind.as_deref())
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.last_observed_at))
    .bind(cursor.as_ref().map(|item| item.row_source_kind.as_str()))
    .bind(cursor.as_ref().map(|item| item.source_actor_id.as_str()))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let last_observed_at: OffsetDateTime = row.get("last_observed_at");
        (
            json!({
                "sourceKind": row.get::<String, _>("source_kind"),
                "sourceActorId": row.get::<String, _>("source_actor_id"),
                "name": row.get::<String, _>("name"),
                "screenName": row.get::<String, _>("screen_name"),
                "lastObservedAt": format_time(last_observed_at),
            }),
            ActorCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                source_kind: source_kind.clone(),
                last_observed_at,
                row_source_kind: row.get("source_kind"),
                source_actor_id: row.get("source_actor_id"),
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_actor(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path((source_kind, source_actor_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let source_kind = normalize_source_kind(&source_kind)?;
    let row = sqlx::query(
        r#"
        SELECT
            a.source_kind,
            a.source_actor_id,
            a.last_observed_at,
            COALESCE(apv.name, '') AS name,
            COALESCE(apv.screen_name, '') AS screen_name,
            apv.avatar_media_id,
            apv.banner_media_id,
            to_jsonb(a) AS record
        FROM actors a
        LEFT JOIN actor_profile_versions apv ON apv.id = a.current_profile_version_id
        WHERE a.source_kind = $1
          AND a.source_actor_id = $2
        "#,
    )
    .bind(&source_kind)
    .bind(source_actor_id.trim())
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("actor not found"))?;

    let current_profile = fetch_optional_json2(
        &state.db,
        r#"
        SELECT to_jsonb(apv)
        FROM actor_profile_versions apv
        INNER JOIN actors a ON a.current_profile_version_id = apv.id
        WHERE a.source_kind = $1
          AND a.source_actor_id = $2
        "#,
        source_kind.as_str(),
        source_actor_id.trim(),
    )
    .await?;
    let latest_metrics = fetch_optional_json2(
        &state.db,
        r#"
        SELECT to_jsonb(t)
        FROM (
            SELECT submission_id, source_kind, source_actor_id, observed_at, followers_count, friends_count, favourites_count, statuses_count, media_count, listed_count, created_at
            FROM actor_metric_observations
            WHERE source_kind = $1
              AND source_actor_id = $2
            ORDER BY observed_at DESC, created_at DESC
            LIMIT 1
        ) t
        "#,
        source_kind.as_str(),
        source_actor_id.trim(),
    )
    .await?;
    let recent_posts = fetch_json_array2(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT source_kind, source_post_id, author_source_actor_id, full_text, last_observed_at
            FROM posts
            WHERE source_kind = $1
              AND author_source_actor_id = $2
            ORDER BY last_observed_at DESC, source_post_id DESC
            LIMIT 10
        ) t
        "#,
        &source_kind,
        source_actor_id.trim(),
    )
    .await?;

    Ok(Json(detail_response(
        json!({
            "sourceKind": row.get::<String, _>("source_kind"),
            "sourceActorId": row.get::<String, _>("source_actor_id"),
            "name": row.get::<String, _>("name"),
            "screenName": row.get::<String, _>("screen_name"),
            "avatarMediaId": row.get::<Option<Uuid>, _>("avatar_media_id"),
            "bannerMediaId": row.get::<Option<Uuid>, _>("banner_media_id"),
            "lastObservedAt": format_time(row.get("last_observed_at")),
        }),
        row.get("record"),
        json!({
            "currentProfile": current_profile,
            "latestMetrics": latest_metrics,
            "recentPosts": recent_posts,
        }),
    )))
}

pub async fn get_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            m.id,
            m.source_kind,
            m.media_family,
            m.identity_kind,
            m.identity_value,
            m.fetch_url,
            m.display_url,
            m.thumb_url,
            m.content_type_hint,
            m.first_observed_at,
            m.last_observed_at,
            j.id AS transfer_job_id,
            j.status AS transfer_status,
            j.storage_object_id,
            o.object_key AS storage_object_key,
            to_jsonb(m) AS record
        FROM managed_media m
        LEFT JOIN media_transfer_jobs j ON j.media_id = m.id
        LEFT JOIN storage_objects o ON o.id = j.storage_object_id
        WHERE m.id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("media not found"))?;

    let sources = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT source_kind, source_media_id, managed_media_id, media_key, source_post_id, media_type, media_url, thumb_url, source_url, width, height, alt_text, allow_download, source_status_id, source_actor_id, duration_ms, first_observed_at, last_observed_at, created_at, updated_at
            FROM post_media_sources
            WHERE managed_media_id = $1
            ORDER BY last_observed_at DESC, source_media_id DESC
            LIMIT 20
        ) t
        "#,
        media_id,
    )
    .await?;
    let transfer_job = fetch_optional_json1(
        &state.db,
        r#"
        SELECT to_jsonb(j)
        FROM media_transfer_jobs j
        WHERE j.media_id = $1
        "#,
        media_id,
    )
    .await?;
    let attempts = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT a.id, a.job_id, a.status, a.upload_mode, a.error, a.bytes_uploaded, a.parts_uploaded, a.started_at, a.finished_at
            FROM media_transfer_attempts a
            INNER JOIN media_transfer_jobs j ON j.id = a.job_id
            WHERE j.media_id = $1
            ORDER BY a.started_at DESC
            LIMIT 20
        ) t
        "#,
        media_id,
    )
    .await?;
    let storage_object = fetch_optional_json1(
        &state.db,
        r#"
        SELECT to_jsonb(o)
        FROM media_storage_bindings b
        INNER JOIN storage_objects o ON o.id = b.storage_object_id
        WHERE b.media_id = $1
          AND b.object_role = 'original'
        LIMIT 1
        "#,
        media_id,
    )
    .await?;

    Ok(Json(detail_response(
        json!({
            "id": row.get::<Uuid, _>("id"),
            "sourceKind": row.get::<String, _>("source_kind"),
            "mediaFamily": row.get::<String, _>("media_family"),
            "identityKind": row.get::<String, _>("identity_kind"),
            "identityValue": row.get::<String, _>("identity_value"),
            "transferStatus": row.get::<Option<String>, _>("transfer_status"),
            "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
            "storageObjectKey": row.get::<Option<String>, _>("storage_object_key"),
            "firstObservedAt": format_time(row.get("first_observed_at")),
            "lastObservedAt": format_time(row.get("last_observed_at")),
        }),
        row.get("record"),
        json!({
            "sources": sources,
            "transferJob": transfer_job,
            "transferAttempts": attempts,
            "storageObject": storage_object,
        }),
    )))
}

pub async fn list_storage_objects(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
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

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT id, provider, bucket, object_key, etag, size_bytes, content_type, created_at
        FROM storage_objects
        WHERE (
                $1::text IS NULL
            OR object_key ILIKE $1
            OR bucket ILIKE $1
            OR provider ILIKE $1
          )
          AND (
                NOT $2
            OR (created_at, id) < ($3, $4)
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $5
        "#,
    )
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let id: Uuid = row.get("id");
        (
            json!({
                "id": id,
                "provider": row.get::<String, _>("provider"),
                "bucket": row.get::<String, _>("bucket"),
                "objectKey": row.get::<String, _>("object_key"),
                "etag": row.get::<Option<String>, _>("etag"),
                "sizeBytes": row.get::<i64, _>("size_bytes"),
                "contentType": row.get::<String, _>("content_type"),
                "createdAt": format_time(created_at),
            }),
            StorageObjectCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_storage_object(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            id,
            provider,
            bucket,
            object_key,
            etag,
            size_bytes,
            content_type,
            created_at,
            to_jsonb(storage_objects) AS record
        FROM storage_objects
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("storage object not found"))?;

    let bindings = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT
                b.id,
                b.media_id,
                b.object_role,
                b.created_at,
                m.source_kind,
                m.media_family,
                m.identity_kind,
                m.identity_value
            FROM media_storage_bindings b
            INNER JOIN managed_media m ON m.id = b.media_id
            WHERE b.storage_object_id = $1
            ORDER BY b.created_at DESC
            LIMIT 20
        ) t
        "#,
        object_id,
    )
    .await?;

    Ok(Json(detail_response(
        json!({
            "id": row.get::<Uuid, _>("id"),
            "provider": row.get::<String, _>("provider"),
            "bucket": row.get::<String, _>("bucket"),
            "objectKey": row.get::<String, _>("object_key"),
            "etag": row.get::<Option<String>, _>("etag"),
            "sizeBytes": row.get::<i64, _>("size_bytes"),
            "contentType": row.get::<String, _>("content_type"),
            "createdAt": format_time(row.get("created_at")),
        }),
        row.get("record"),
        json!({
            "bindings": bindings,
        }),
    )))
}

pub async fn sign_storage_object(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT id, bucket, object_key
        FROM storage_objects
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("storage object not found"))?;

    let bucket: String = row.get("bucket");
    let object_key: String = row.get("object_key");
    let expires_at = OffsetDateTime::now_utc() + time::Duration::minutes(30);
    let client = storage::build_s3_client(&state).await?;
    let url = storage::presign_get_object_url(
        &client,
        &bucket,
        &object_key,
        std::time::Duration::from_secs(30 * 60),
    )
    .await?;

    db::insert_audit_event(
        &state.db,
        admin.record.user_id,
        "storage.signed_url_requested",
        "storage_object",
        Some(object_id.to_string()),
        json!({
            "bucket": bucket,
            "object_key": object_key,
            "expires_at": format_time(expires_at),
        }),
    )
    .await?;

    Ok(Json(json!({
        "url": url,
        "expiresAt": format_time(expires_at),
    })))
}

pub async fn transfer_overview(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let counts = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (WHERE status = 'processing') AS processing,
            COUNT(*) FILTER (WHERE status = 'retryable') AS retryable,
            COUNT(*) FILTER (WHERE status = 'succeeded') AS succeeded,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed
        FROM media_transfer_jobs
        "#,
    )
    .fetch_one(&state.db)
    .await?;
    let recent_failed_jobs = fetch_transfer_job_array(
        &state.db,
        r#"
        SELECT
            j.id,
            j.media_id,
            j.source_kind,
            j.status,
            j.attempt_count,
            j.next_run_at,
            j.lease_expires_at,
            j.updated_at,
            j.fetch_url,
            o.id AS storage_object_id,
            o.object_key
        FROM media_transfer_jobs j
        LEFT JOIN storage_objects o ON o.id = j.storage_object_id
        WHERE j.status IN ('failed', 'retryable')
        ORDER BY j.updated_at DESC, j.id DESC
        LIMIT 10
        "#,
    )
    .await?;
    let recent_attempts = fetch_json_array_no_args(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT id, job_id, status, upload_mode, error, bytes_uploaded, parts_uploaded, started_at, finished_at
            FROM media_transfer_attempts
            ORDER BY started_at DESC
            LIMIT 10
        ) t
        "#,
    )
    .await?;

    Ok(Json(json!({
        "enabled": state.settings.config.transfer.enabled,
        "config": {
            "workerCount": state.settings.config.transfer.worker_count,
            "chunkSizeMb": state.settings.config.transfer.chunk_size_mb,
            "downloadParallelism": state.settings.config.transfer.download_parallelism,
            "uploadParallelism": state.settings.config.transfer.upload_parallelism,
            "maxInFlightParts": state.settings.config.transfer.max_in_flight_parts,
            "maxAttempts": state.settings.config.transfer.max_attempts,
            "workerPollIntervalSeconds": state.settings.config.transfer.worker_poll_interval_seconds,
            "attemptTimeoutSeconds": state.settings.config.transfer.attempt_timeout_seconds,
        },
        "statusCounts": {
            "pending": counts.get::<i64, _>("pending"),
            "processing": counts.get::<i64, _>("processing"),
            "retryable": counts.get::<i64, _>("retryable"),
            "succeeded": counts.get::<i64, _>("succeeded"),
            "failed": counts.get::<i64, _>("failed"),
        },
        "recentFailedJobs": recent_failed_jobs,
        "recentAttempts": recent_attempts,
    })))
}

pub async fn list_transfer_jobs(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TransferJobListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let status = normalize_transfer_status(query.status.as_deref())?;
    let cursor = decode_cursor::<TransferJobCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        ensure_filter_match(
            cursor.status.as_deref(),
            status.as_deref(),
            "cursor does not match status filter",
        )?;
    }

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            j.id,
            j.media_id,
            j.source_kind,
            j.status,
            j.attempt_count,
            j.next_run_at,
            j.lease_expires_at,
            j.updated_at,
            j.fetch_url,
            o.id AS storage_object_id,
            o.object_key
        FROM media_transfer_jobs j
        LEFT JOIN storage_objects o ON o.id = j.storage_object_id
        WHERE ($1::text IS NULL OR j.status = $1)
          AND (
                $2::text IS NULL
            OR j.media_id::text ILIKE $2
            OR j.fetch_url ILIKE $2
            OR COALESCE(o.object_key, '') ILIKE $2
          )
          AND (
                NOT $3
            OR (j.updated_at, j.id) < ($4, $5)
          )
        ORDER BY j.updated_at DESC, j.id DESC
        LIMIT $6
        "#,
    )
    .bind(status.as_deref())
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.updated_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: Uuid = row.get("id");
        (
            transfer_job_json_from_row(&row),
            TransferJobCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_transfer_job(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(job_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            j.id,
            j.media_id,
            j.source_kind,
            j.status,
            j.attempt_count,
            j.next_run_at,
            j.lease_expires_at,
            j.updated_at,
            j.fetch_url,
            j.last_error,
            o.id AS storage_object_id,
            o.object_key,
            to_jsonb(j) AS record
        FROM media_transfer_jobs j
        LEFT JOIN storage_objects o ON o.id = j.storage_object_id
        WHERE j.id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer job not found"))?;

    let attempts = fetch_json_array(
        &state.db,
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb)
        FROM (
            SELECT id, job_id, status, upload_mode, error, bytes_uploaded, parts_uploaded, started_at, finished_at
            FROM media_transfer_attempts
            WHERE job_id = $1
            ORDER BY started_at DESC
            LIMIT 20
        ) t
        "#,
        job_id,
    )
    .await?;
    let managed_media = fetch_optional_json1(
        &state.db,
        r#"
        SELECT to_jsonb(m)
        FROM managed_media m
        INNER JOIN media_transfer_jobs j ON j.media_id = m.id
        WHERE j.id = $1
        "#,
        job_id,
    )
    .await?;

    Ok(Json(detail_response(
        transfer_job_json_from_row(&row),
        row.get("record"),
        json!({
            "managedMedia": managed_media,
            "attempts": attempts,
        }),
    )))
}

pub async fn requeue_transfer_job(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(job_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT id, media_id, status
        FROM media_transfer_jobs
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *tx)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer job not found"))?;
    let previous_status: String = row.get("status");

    match previous_status.as_str() {
        "failed" | "retryable" | "succeeded" => {}
        _ => return Err(AppError::bad_request("job status does not support requeue")),
    }

    sqlx::query(
        r#"
        UPDATE media_transfer_jobs
        SET status = 'pending',
            next_run_at = NOW(),
            leased_at = NULL,
            lease_expires_at = NULL,
            last_error = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await?;

    db::insert_audit_event_tx(
        &mut tx,
        admin.record.user_id,
        "transfer.requeued",
        "media_transfer_job",
        Some(job_id.to_string()),
        json!({
            "previous_status": previous_status,
            "media_id": row.get::<Uuid, _>("media_id"),
        }),
    )
    .await?;

    tx.commit().await?;
    let job = fetch_transfer_job_summary(&state.db, job_id).await?;

    Ok(Json(json!({
        "ok": true,
        "job": job,
    })))
}

fn transfer_job_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    let status = row.get::<String, _>("status");
    json!({
        "id": row.get::<Uuid, _>("id"),
        "mediaId": row.get::<Uuid, _>("media_id"),
        "sourceKind": row.get::<String, _>("source_kind"),
        "status": status,
        "attemptCount": row.get::<i32, _>("attempt_count"),
        "nextRunAt": format_time(row.get("next_run_at")),
        "leaseExpiresAt": format_time_opt(row.get("lease_expires_at")),
        "updatedAt": format_time(row.get("updated_at")),
        "fetchUrl": row.get::<String, _>("fetch_url"),
        "lastError": row.try_get::<Option<String>, _>("last_error").ok().flatten(),
        "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
        "storageObjectKey": row.get::<Option<String>, _>("object_key"),
        "canRequeue": matches!(status.as_str(), "failed" | "retryable" | "succeeded"),
    })
}

fn fetch_post_media_json<'a>(
    pool: &'a PgPool,
    source_kind: &'a str,
    source_post_id: &'a str,
) -> impl std::future::Future<Output = AppResult<Value>> + 'a {
    async move {
        let rows = sqlx::query(
            r#"
            SELECT
                pm.position,
                pms.source_media_id,
                pms.media_type,
                pms.media_url,
                pms.thumb_url,
                pms.source_url,
                pms.width,
                pms.height,
                pms.alt_text,
                pms.duration_ms,
                pms.managed_media_id,
                j.status AS transfer_status,
                o.id AS storage_object_id,
                o.object_key
            FROM post_media pm
            INNER JOIN post_media_sources pms
                ON pms.source_kind = pm.source_kind
               AND pms.source_media_id = pm.source_media_id
            LEFT JOIN media_transfer_jobs j ON j.media_id = pms.managed_media_id
            LEFT JOIN storage_objects o ON o.id = j.storage_object_id
            WHERE pm.source_kind = $1
              AND pm.source_post_id = $2
            ORDER BY pm.position ASC, pms.source_media_id ASC
            "#,
        )
        .bind(source_kind)
        .bind(source_post_id)
        .fetch_all(pool)
        .await?;

        Ok(Value::Array(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "position": row.get::<i32, _>("position"),
                        "sourceMediaId": row.get::<String, _>("source_media_id"),
                        "mediaType": row.get::<String, _>("media_type"),
                        "mediaUrl": row.get::<String, _>("media_url"),
                        "thumbUrl": row.get::<String, _>("thumb_url"),
                        "sourceUrl": row.get::<String, _>("source_url"),
                        "width": row.get::<i32, _>("width"),
                        "height": row.get::<i32, _>("height"),
                        "altText": row.get::<Option<String>, _>("alt_text"),
                        "durationMs": row.get::<Option<i64>, _>("duration_ms"),
                        "managedMediaId": row.get::<Uuid, _>("managed_media_id"),
                        "transferStatus": row.get::<Option<String>, _>("transfer_status"),
                        "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
                        "storageObjectKey": row.get::<Option<String>, _>("object_key"),
                    })
                })
                .collect(),
        ))
    }
}

async fn fetch_transfer_job_array(pool: &PgPool, sql: &str) -> AppResult<Value> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| transfer_job_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_transfer_job_summary(pool: &PgPool, job_id: Uuid) -> AppResult<Value> {
    let row = sqlx::query(
        r#"
        SELECT
            j.id,
            j.media_id,
            j.source_kind,
            j.status,
            j.attempt_count,
            j.next_run_at,
            j.lease_expires_at,
            j.updated_at,
            j.fetch_url,
            j.last_error,
            o.id AS storage_object_id,
            o.object_key
        FROM media_transfer_jobs j
        LEFT JOIN storage_objects o ON o.id = j.storage_object_id
        WHERE j.id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer job not found"))?;
    Ok(transfer_job_json_from_row(&row))
}

async fn fetch_user_summary_json_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> AppResult<Value> {
    let row = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM users
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

async fn fetch_json_array<'a, T>(pool: &PgPool, sql: &'a str, arg: T) -> AppResult<Value>
where
    T: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
{
    let value = sqlx::query_scalar::<_, Value>(sql)
        .bind(arg)
        .fetch_one(pool)
        .await?;
    Ok(value)
}

async fn fetch_json_array2<'a, T1, T2>(
    pool: &PgPool,
    sql: &'a str,
    arg1: T1,
    arg2: T2,
) -> AppResult<Value>
where
    T1: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
    T2: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
{
    let value = sqlx::query_scalar::<_, Value>(sql)
        .bind(arg1)
        .bind(arg2)
        .fetch_one(pool)
        .await?;
    Ok(value)
}

async fn fetch_json_array_no_args(pool: &PgPool, sql: &str) -> AppResult<Value> {
    let value = sqlx::query_scalar::<_, Value>(sql).fetch_one(pool).await?;
    Ok(value)
}

async fn fetch_optional_json1<'a, T>(
    pool: &PgPool,
    sql: &'a str,
    arg: T,
) -> AppResult<Option<Value>>
where
    T: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
{
    sqlx::query_scalar::<_, Value>(sql)
        .bind(arg)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

async fn fetch_optional_json2<'a, T1, T2>(
    pool: &PgPool,
    sql: &'a str,
    arg1: T1,
    arg2: T2,
) -> AppResult<Option<Value>>
where
    T1: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
    T2: Send + 'a + sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>,
{
    sqlx::query_scalar::<_, Value>(sql)
        .bind(arg1)
        .bind(arg2)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

fn list_response(items: Vec<Value>, next_cursor: Option<String>) -> Value {
    json!({
        "items": items,
        "nextCursor": next_cursor,
    })
}

fn detail_response(summary: Value, record: Value, related: Value) -> Value {
    json!({
        "summary": summary,
        "record": record,
        "related": related,
    })
}

fn resolve_limit(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn limit_plus_one(limit: usize) -> i64 {
    (limit.saturating_add(1)) as i64
}

fn normalize_query(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn like_pattern(value: Option<&str>) -> Option<String> {
    value.map(|value| format!("%{value}%"))
}

fn normalize_source_kind(raw: &str) -> AppResult<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(AppError::bad_request("sourceKind is required"));
    }
    Ok(value)
}

fn normalize_optional_source_kind(raw: Option<&str>) -> AppResult<Option<String>> {
    raw.map(normalize_source_kind).transpose()
}

fn normalize_user_status(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "active" | "disabled" => Ok(value),
        _ => Err(AppError::bad_request(
            "status must be one of all, active, disabled",
        )),
    }
}

fn normalize_transfer_status(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "pending" | "processing" | "retryable" | "succeeded" | "failed" => Ok(Some(value)),
        "" => Ok(None),
        _ => Err(AppError::bad_request(
            "status must be one of pending, processing, retryable, succeeded, failed",
        )),
    }
}

fn ensure_cursor_version(version: u8) -> AppResult<()> {
    if version == CURSOR_VERSION {
        Ok(())
    } else {
        Err(AppError::bad_request("unsupported cursor version"))
    }
}

fn ensure_filter_match(
    cursor_value: Option<&str>,
    request_value: Option<&str>,
    message: &str,
) -> AppResult<()> {
    if cursor_value == request_value {
        Ok(())
    } else {
        Err(AppError::bad_request(message))
    }
}

fn paginate_rows<T, C, F>(
    mut rows: Vec<sqlx::postgres::PgRow>,
    limit: usize,
    map: F,
) -> AppResult<(Vec<T>, Option<String>)>
where
    C: Serialize,
    F: Fn(sqlx::postgres::PgRow) -> (T, C),
{
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    let mut items = Vec::with_capacity(rows.len());
    let mut last_cursor = None;

    for row in rows {
        let (item, cursor) = map(row);
        items.push(item);
        last_cursor = Some(encode_cursor(&cursor)?);
    }

    Ok((items, has_more.then_some(last_cursor).flatten()))
}

fn encode_cursor<T: Serialize>(value: &T) -> AppResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor<T: DeserializeOwned>(raw: Option<&str>) -> AppResult<Option<T>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| AppError::bad_request("invalid cursor"))?;
    let cursor =
        serde_json::from_slice(&bytes).map_err(|_| AppError::bad_request("invalid cursor"))?;
    Ok(Some(cursor))
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

fn format_time_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_cursor_payload() {
        let cursor = UserCursor {
            v: CURSOR_VERSION,
            q: Some("demo".to_owned()),
            status: "active".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            id: Uuid::now_v7(),
        };

        let encoded = encode_cursor(&cursor).unwrap();
        let decoded = decode_cursor::<UserCursor>(Some(&encoded)).unwrap();
        assert_eq!(decoded.unwrap().status, "active");
    }

    #[test]
    fn rejects_invalid_cursor_payload() {
        let error = decode_cursor::<UserCursor>(Some("not-a-valid-cursor")).unwrap_err();
        assert_eq!(error.to_string(), "invalid cursor");
    }

    #[test]
    fn rejects_unknown_user_status_filter() {
        let error = normalize_user_status(Some("paused")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "status must be one of all, active, disabled"
        );
    }
}
