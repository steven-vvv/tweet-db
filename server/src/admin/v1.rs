use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    response::Redirect,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{collections::HashMap, time::Duration};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    db,
    error::{AppError, AppResult},
    search::{SearchHit, SearchSort, SearchState, TweetSearchFilters},
    state::AppState,
    storage,
};

pub(super) const DEFAULT_LIMIT: usize = 50;
pub(super) const MAX_LIMIT: usize = 100;
pub(super) const CURSOR_VERSION: u8 = 1;

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
pub struct TwitterUserListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TweetListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub author_id: Option<String>,
    pub relation: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub media_type: Option<String>,
    pub transfer_status: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StorageObjectListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferTaskListQuery {
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

#[derive(Debug, Serialize, Deserialize)]
struct TwitterUserCursor {
    v: u8,
    q: Option<String>,
    #[serde(with = "cursor_datetime")]
    updated_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TwitterUserSearchCursor {
    v: u8,
    q: String,
    sort: SearchSort,
    offset: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetCursor {
    v: u8,
    q: Option<String>,
    author_id: Option<i64>,
    relation: String,
    #[serde(with = "cursor_datetime")]
    published_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetSearchCursor {
    v: u8,
    q: String,
    sort: SearchSort,
    author_id: Option<i64>,
    relation: String,
    offset: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaCursor {
    v: u8,
    q: Option<String>,
    media_type: String,
    transfer_status: String,
    #[serde(with = "cursor_datetime")]
    updated_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct StorageObjectCursor {
    v: u8,
    q: Option<String>,
    #[serde(with = "cursor_datetime")]
    created_at: OffsetDateTime,
    id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct TransferTaskCursor {
    v: u8,
    q: Option<String>,
    status: String,
    #[serde(with = "cursor_datetime")]
    updated_at: OffsetDateTime,
    id: Uuid,
}

use super::{cursor_datetime, support::*};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

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

pub async fn overview(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;

    let account_counts = sqlx::query(
        r#"
        SELECT
            COUNT(*) AS total,
            COUNT(*) FILTER (WHERE disabled_at IS NULL) AS active,
            COUNT(*) FILTER (WHERE disabled_at IS NOT NULL) AS disabled,
            COUNT(*) FILTER (WHERE is_admin) AS admins
        FROM iam.users
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let domain_counts = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM tweet.twitter_user) AS twitter_users,
            (SELECT COUNT(*) FROM tweet.tweet) AS tweets,
            (SELECT COUNT(*) FROM tweet.media) AS media,
            (SELECT COUNT(*) FROM media.storage_object) AS storage_objects
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let transfer_counts = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (WHERE status = 'processing') AS processing,
            COUNT(*) FILTER (WHERE status = 'completed') AS completed,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed,
            COUNT(*) FILTER (WHERE status = 'canceled') AS canceled
        FROM media.transfer_task
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let recent_tweets = fetch_recent_tweets(&state.db).await?;
    let recent_failed_tasks = fetch_transfer_task_array(
        &state.db,
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
        WHERE task.status IN ('failed', 'canceled')
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT 10
        "#,
    )
    .await?;

    Ok(Json(json!({
        "accounts": {
            "total": account_counts.get::<i64, _>("total"),
            "active": account_counts.get::<i64, _>("active"),
            "disabled": account_counts.get::<i64, _>("disabled"),
            "admins": account_counts.get::<i64, _>("admins"),
        },
        "domain": {
            "twitterUsers": domain_counts.get::<i64, _>("twitter_users"),
            "tweets": domain_counts.get::<i64, _>("tweets"),
            "media": domain_counts.get::<i64, _>("media"),
            "storageObjects": domain_counts.get::<i64, _>("storage_objects"),
        },
        "transfer": {
            "enabled": state.settings.config.transfer.enabled,
            "workerCount": state.settings.config.transfer.worker_count,
            "pending": transfer_counts.get::<i64, _>("pending"),
            "processing": transfer_counts.get::<i64, _>("processing"),
            "completed": transfer_counts.get::<i64, _>("completed"),
            "failed": transfer_counts.get::<i64, _>("failed"),
            "canceled": transfer_counts.get::<i64, _>("canceled"),
        },
        "storage": {
            "provider": state.settings.config.storage.provider,
            "bucket": state.settings.config.storage.bucket,
        },
        "recentTweets": recent_tweets,
        "recentFailedTasks": recent_failed_tasks,
    })))
}

pub async fn list_twitter_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TwitterUserListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let sort = SearchSort::parse(query.sort.as_deref(), q.is_some())?;
    if let (Some(search), Some(search_query)) = (state.search.as_ref(), q.as_deref()) {
        let value = list_twitter_users_search(
            &state.db,
            search,
            search_query,
            sort,
            limit,
            query.list.cursor.as_deref(),
        )
        .await?;
        return Ok(Json(value));
    }

    let cursor = decode_cursor::<TwitterUserCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let q_lower_prefix = lowercase_prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            u.id,
            u.registered_at,
            u.created_at,
            u.updated_at,
            snapshot.display_name,
            snapshot.user_name,
            snapshot.avatar_url,
            stats.followers,
            stats.following,
            stats.tweets,
            COALESCE(tweet_counts.tweet_count, 0) AS tweet_count,
            COALESCE(media_counts.media_count, 0) AS media_count
        FROM tweet.twitter_user AS u
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = u.id
        LEFT JOIN tweet.v_latest_user_stats AS stats
          ON stats.user_id = u.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS tweet_count
            FROM tweet.tweet AS tweet
            WHERE tweet.author_id = u.id
        ) AS tweet_counts ON TRUE
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM tweet.media AS media
            WHERE media.origin_user_id = u.id
        ) AS media_counts ON TRUE
        WHERE (
                $1::text IS NULL
            OR u.id::text LIKE $1
            OR lower(COALESCE(snapshot.user_name, '')) LIKE $2
            OR lower(COALESCE(snapshot.display_name, '')) LIKE $2
        )
          AND (
                NOT $3
            OR (u.updated_at, u.id) < ($4, $5)
          )
        ORDER BY u.updated_at DESC, u.id DESC
        LIMIT $6
        "#,
    )
    .bind(q_prefix.as_deref())
    .bind(q_lower_prefix.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.updated_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: i64 = row.get("id");
        (
            json!({
                "id": id.to_string(),
                "displayName": row.get::<Option<String>, _>("display_name"),
                "userName": row.get::<Option<String>, _>("user_name"),
                "avatarUrl": row.get::<Option<String>, _>("avatar_url"),
                "followers": row.get::<Option<i64>, _>("followers"),
                "following": row.get::<Option<i64>, _>("following"),
                "tweets": row.get::<Option<i64>, _>("tweets"),
                "savedTweets": row.get::<i64, _>("tweet_count"),
                "savedMedia": row.get::<i64, _>("media_count"),
                "registeredAt": format_time_opt(row.get("registered_at")),
                "updatedAt": format_time(updated_at),
            }),
            TwitterUserCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

async fn list_twitter_users_search(
    pool: &PgPool,
    search: &SearchState,
    q: &str,
    sort: SearchSort,
    limit: usize,
    raw_cursor: Option<&str>,
) -> AppResult<Value> {
    let cursor = decode_cursor::<TwitterUserSearchCursor>(raw_cursor)?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            Some(cursor.q.as_str()),
            Some(q),
            "cursor does not match query",
        )?;
        if cursor.sort != sort {
            return Err(AppError::bad_request("cursor does not match sort"));
        }
    }

    let offset = cursor.as_ref().map(|item| item.offset).unwrap_or_default();
    let mut hits = search
        .search_users(Some(q), sort, limit.saturating_add(1), offset)
        .await?;
    let has_more = hits.len() > limit;
    if has_more {
        hits.truncate(limit);
    }

    let items = fetch_twitter_user_search_items(pool, &hits).await?;
    let next_cursor = if has_more {
        Some(encode_cursor(&TwitterUserSearchCursor {
            v: CURSOR_VERSION,
            q: q.to_owned(),
            sort,
            offset: offset.saturating_add(limit),
        })?)
    } else {
        None
    };

    Ok(list_response(items, next_cursor))
}

async fn fetch_twitter_user_search_items(
    pool: &PgPool,
    hits: &[SearchHit],
) -> AppResult<Vec<Value>> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let ids = hits.iter().map(|hit| hit.id).collect::<Vec<_>>();
    let rows = sqlx::query(
        r#"
        SELECT
            u.id,
            u.registered_at,
            u.updated_at,
            snapshot.display_name,
            snapshot.user_name,
            snapshot.avatar_url,
            stats.followers,
            stats.following,
            stats.tweets,
            COALESCE(tweet_counts.tweet_count, 0) AS tweet_count,
            COALESCE(media_counts.media_count, 0) AS media_count
        FROM tweet.twitter_user AS u
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = u.id
        LEFT JOIN tweet.v_latest_user_stats AS stats
          ON stats.user_id = u.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS tweet_count
            FROM tweet.tweet AS tweet
            WHERE tweet.author_id = u.id
        ) AS tweet_counts ON TRUE
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM tweet.media AS media
            WHERE media.origin_user_id = u.id
        ) AS media_counts ON TRUE
        WHERE u.id = ANY($1::BIGINT[])
        "#,
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;

    let mut by_id = HashMap::<i64, Value>::new();
    for row in rows {
        let id: i64 = row.get("id");
        by_id.insert(
            id,
            json!({
                "id": id.to_string(),
                "displayName": row.get::<Option<String>, _>("display_name"),
                "userName": row.get::<Option<String>, _>("user_name"),
                "avatarUrl": row.get::<Option<String>, _>("avatar_url"),
                "followers": row.get::<Option<i64>, _>("followers"),
                "following": row.get::<Option<i64>, _>("following"),
                "tweets": row.get::<Option<i64>, _>("tweets"),
                "savedTweets": row.get::<i64, _>("tweet_count"),
                "savedMedia": row.get::<i64, _>("media_count"),
                "registeredAt": format_time_opt(row.get("registered_at")),
                "updatedAt": format_time(row.get("updated_at")),
            }),
        );
    }

    Ok(hits
        .iter()
        .filter_map(|hit| {
            by_id.remove(&hit.id).map(|mut item| {
                if let Some(object) = item.as_object_mut() {
                    object.insert("searchScore".to_owned(), json!(hit.score));
                    object.insert("searchSortTime".to_owned(), json!(hit.sort_time));
                }
                item
            })
        })
        .collect())
}

pub async fn get_twitter_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            u.id,
            u.registered_at,
            u.created_at,
            u.updated_at,
            to_jsonb(u) AS record,
            to_jsonb(snapshot) AS snapshot,
            to_jsonb(stats) AS stats,
            COALESCE(tweet_counts.tweet_count, 0) AS tweet_count,
            COALESCE(media_counts.media_count, 0) AS media_count
        FROM tweet.twitter_user AS u
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = u.id
        LEFT JOIN tweet.v_latest_user_stats AS stats
          ON stats.user_id = u.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS tweet_count
            FROM tweet.tweet AS tweet
            WHERE tweet.author_id = u.id
        ) AS tweet_counts ON TRUE
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM tweet.media AS media
            WHERE media.origin_user_id = u.id
        ) AS media_counts ON TRUE
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("twitter user not found"))?;
    let snapshot = row.get::<Option<Value>, _>("snapshot");
    let stats = row.get::<Option<Value>, _>("stats");
    let recent_tweets = fetch_user_tweets(&state.db, user_id).await?;
    let media = fetch_user_media(&state.db, user_id).await?;

    Ok(Json(detail_response(
        json!({
            "id": user_id.to_string(),
            "displayName": snapshot.as_ref().and_then(|value| value.get("display_name")).cloned(),
            "userName": snapshot.as_ref().and_then(|value| value.get("user_name")).cloned(),
            "tweetCount": row.get::<i64, _>("tweet_count"),
            "mediaCount": row.get::<i64, _>("media_count"),
            "registeredAt": format_time_opt(row.get("registered_at")),
            "updatedAt": format_time(row.get("updated_at")),
        }),
        row.get("record"),
        json!({
            "snapshot": snapshot,
            "stats": stats,
            "recentTweets": recent_tweets,
            "media": media,
        }),
    )))
}

pub async fn list_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TweetListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let author_id = parse_optional_i64(query.author_id.as_deref(), "authorId")?;
    let relation = normalize_tweet_relation(query.relation.as_deref())?;
    let sort = SearchSort::parse(query.sort.as_deref(), q.is_some())?;
    if let (Some(search), Some(search_query)) = (state.search.as_ref(), q.as_deref()) {
        let value = list_tweets_search(
            &state.db,
            search,
            search_query,
            sort,
            author_id,
            &relation,
            limit,
            query.list.cursor.as_deref(),
        )
        .await?;
        return Ok(Json(value));
    }

    let cursor = decode_cursor::<TweetCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        if cursor.author_id != author_id || cursor.relation != relation {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            t.id,
            t.published_at,
            t.author_id,
            t.conversation_id,
            t.reply_to_tweet_id,
            t.quote_tweet_id,
            t.repost_id,
            COALESCE((t.note_text).body, (t.legacy_text).body) AS text_body,
            author_snapshot.user_name AS author_user_name,
            author_snapshot.display_name AS author_display_name,
            stats.views,
            stats.replies,
            stats.reposts,
            stats.quotes,
            stats.likes,
            COALESCE(media_counts.media_count, 0) AS media_count
        FROM tweet.tweet AS t
        LEFT JOIN tweet.v_latest_user_snapshot AS author_snapshot
          ON author_snapshot.user_id = t.author_id
        LEFT JOIN tweet.v_latest_tweet_stats AS stats
          ON stats.tweet_id = t.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM tweet.tweet_media_ref AS ref
            WHERE ref.tweet_id = t.id
        ) AS media_counts ON TRUE
        WHERE ($1::BIGINT IS NULL OR t.author_id = $1)
          AND (
                $2::text = 'all'
            OR ($2 = 'original' AND t.reply_to_tweet_id IS NULL AND t.quote_tweet_id IS NULL AND t.repost_id IS NULL)
            OR ($2 = 'reply' AND t.reply_to_tweet_id IS NOT NULL)
            OR ($2 = 'quote' AND t.quote_tweet_id IS NOT NULL)
            OR ($2 = 'repost' AND t.repost_id IS NOT NULL)
          )
          AND (
                $3::text IS NULL
            OR t.id::text LIKE $3
            OR t.author_id::text LIKE $3
          )
          AND (
                NOT $4
            OR (t.published_at, t.id) < ($5, $6)
          )
        ORDER BY t.published_at DESC, t.id DESC
        LIMIT $7
        "#,
    )
    .bind(author_id)
    .bind(&relation)
    .bind(q_prefix.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.published_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let published_at: OffsetDateTime = row.get("published_at");
        let id: i64 = row.get("id");
        let text_body: String = row.get("text_body");
        (
            json!({
                "id": id.to_string(),
                "publishedAt": format_time(published_at),
                "authorId": row.get::<i64, _>("author_id").to_string(),
                "authorUserName": row.get::<Option<String>, _>("author_user_name"),
                "authorDisplayName": row.get::<Option<String>, _>("author_display_name"),
                "text": truncate_text(&text_body, 180),
                "conversationId": row.get::<i64, _>("conversation_id").to_string(),
                "replyToTweetId": row.get::<Option<i64>, _>("reply_to_tweet_id").map(|value| value.to_string()),
                "quoteTweetId": row.get::<Option<i64>, _>("quote_tweet_id").map(|value| value.to_string()),
                "repostId": row.get::<Option<i64>, _>("repost_id").map(|value| value.to_string()),
                "views": row.get::<Option<i64>, _>("views").map(|value| value.to_string()),
                "replies": row.get::<Option<i64>, _>("replies"),
                "reposts": row.get::<Option<i64>, _>("reposts"),
                "quotes": row.get::<Option<i64>, _>("quotes"),
                "likes": row.get::<Option<i64>, _>("likes"),
                "mediaCount": row.get::<i64, _>("media_count"),
            }),
            TweetCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                author_id,
                relation: relation.clone(),
                published_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

async fn list_tweets_search(
    pool: &PgPool,
    search: &SearchState,
    q: &str,
    sort: SearchSort,
    author_id: Option<i64>,
    relation: &str,
    limit: usize,
    raw_cursor: Option<&str>,
) -> AppResult<Value> {
    let cursor = decode_cursor::<TweetSearchCursor>(raw_cursor)?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            Some(cursor.q.as_str()),
            Some(q),
            "cursor does not match query",
        )?;
        if cursor.sort != sort || cursor.author_id != author_id || cursor.relation != relation {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let offset = cursor.as_ref().map(|item| item.offset).unwrap_or_default();
    let filters = TweetSearchFilters {
        author_id,
        relation: Some(relation.to_owned()),
    };
    let mut hits = search
        .search_tweets(Some(q), &filters, sort, limit.saturating_add(1), offset)
        .await?;
    let has_more = hits.len() > limit;
    if has_more {
        hits.truncate(limit);
    }

    let items = fetch_tweet_search_items(pool, &hits).await?;
    let next_cursor = if has_more {
        Some(encode_cursor(&TweetSearchCursor {
            v: CURSOR_VERSION,
            q: q.to_owned(),
            sort,
            author_id,
            relation: relation.to_owned(),
            offset: offset.saturating_add(limit),
        })?)
    } else {
        None
    };

    Ok(list_response(items, next_cursor))
}

async fn fetch_tweet_search_items(pool: &PgPool, hits: &[SearchHit]) -> AppResult<Vec<Value>> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let ids = hits.iter().map(|hit| hit.id).collect::<Vec<_>>();
    let rows = sqlx::query(
        r#"
        SELECT
            t.id,
            t.published_at,
            t.author_id,
            t.conversation_id,
            t.reply_to_tweet_id,
            t.quote_tweet_id,
            t.repost_id,
            COALESCE((t.note_text).body, (t.legacy_text).body) AS text_body,
            author_snapshot.user_name AS author_user_name,
            author_snapshot.display_name AS author_display_name,
            stats.views,
            stats.replies,
            stats.reposts,
            stats.quotes,
            stats.likes,
            COALESCE(media_counts.media_count, 0) AS media_count
        FROM tweet.tweet AS t
        LEFT JOIN tweet.v_latest_user_snapshot AS author_snapshot
          ON author_snapshot.user_id = t.author_id
        LEFT JOIN tweet.v_latest_tweet_stats AS stats
          ON stats.tweet_id = t.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS media_count
            FROM tweet.tweet_media_ref AS ref
            WHERE ref.tweet_id = t.id
        ) AS media_counts ON TRUE
        WHERE t.id = ANY($1::BIGINT[])
        "#,
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;

    let mut by_id = HashMap::<i64, Value>::new();
    for row in rows {
        let id: i64 = row.get("id");
        let published_at: OffsetDateTime = row.get("published_at");
        let text_body: String = row.get("text_body");
        by_id.insert(
            id,
            json!({
                "id": id.to_string(),
                "publishedAt": format_time(published_at),
                "authorId": row.get::<i64, _>("author_id").to_string(),
                "authorUserName": row.get::<Option<String>, _>("author_user_name"),
                "authorDisplayName": row.get::<Option<String>, _>("author_display_name"),
                "text": truncate_text(&text_body, 180),
                "conversationId": row.get::<i64, _>("conversation_id").to_string(),
                "replyToTweetId": row.get::<Option<i64>, _>("reply_to_tweet_id").map(|value| value.to_string()),
                "quoteTweetId": row.get::<Option<i64>, _>("quote_tweet_id").map(|value| value.to_string()),
                "repostId": row.get::<Option<i64>, _>("repost_id").map(|value| value.to_string()),
                "views": row.get::<Option<i64>, _>("views").map(|value| value.to_string()),
                "replies": row.get::<Option<i64>, _>("replies"),
                "reposts": row.get::<Option<i64>, _>("reposts"),
                "quotes": row.get::<Option<i64>, _>("quotes"),
                "likes": row.get::<Option<i64>, _>("likes"),
                "mediaCount": row.get::<i64, _>("media_count"),
            }),
        );
    }

    Ok(hits
        .iter()
        .filter_map(|hit| {
            by_id.remove(&hit.id).map(|mut item| {
                if let Some(object) = item.as_object_mut() {
                    object.insert("searchScore".to_owned(), json!(hit.score));
                    object.insert("searchSortTime".to_owned(), json!(hit.sort_time));
                }
                item
            })
        })
        .collect())
}

pub async fn get_tweet(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(tweet_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            t.id,
            t.published_at,
            t.author_id,
            (t.legacy_text).body AS text_body,
            to_jsonb(t) AS record,
            to_jsonb(stats) AS stats,
            to_jsonb(edit) AS edit,
            to_jsonb(policy) AS policy,
            to_jsonb(note) AS community_note,
            to_jsonb(place) AS place,
            to_jsonb(author_snapshot) AS author_snapshot
        FROM tweet.tweet AS t
        LEFT JOIN tweet.v_latest_tweet_stats AS stats
          ON stats.tweet_id = t.id
        LEFT JOIN tweet.tweet_edit AS edit
          ON edit.tweet_id = t.id
        LEFT JOIN tweet.tweet_policy AS policy
          ON policy.tweet_id = t.id
        LEFT JOIN tweet.tweet_community_note AS note
          ON note.tweet_id = t.id
        LEFT JOIN tweet.tweet_place AS place
          ON place.id = t.place_id
        LEFT JOIN tweet.v_latest_user_snapshot AS author_snapshot
          ON author_snapshot.user_id = t.author_id
        WHERE t.id = $1
        "#,
    )
    .bind(tweet_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("tweet not found"))?;
    let media = fetch_tweet_media(&state.db, tweet_id).await?;

    Ok(Json(detail_response(
        json!({
            "id": tweet_id.to_string(),
            "publishedAt": format_time(row.get("published_at")),
            "authorId": row.get::<i64, _>("author_id").to_string(),
            "text": row.get::<String, _>("text_body"),
        }),
        row.get("record"),
        json!({
            "stats": row.get::<Option<Value>, _>("stats"),
            "edit": row.get::<Option<Value>, _>("edit"),
            "policy": row.get::<Option<Value>, _>("policy"),
            "communityNote": row.get::<Option<Value>, _>("community_note"),
            "place": row.get::<Option<Value>, _>("place"),
            "author": row.get::<Option<Value>, _>("author_snapshot"),
            "media": media,
        }),
    )))
}

pub async fn list_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<MediaListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let media_type = normalize_media_type(query.media_type.as_deref())?;
    let transfer_status = normalize_transfer_status(query.transfer_status.as_deref())?;
    let cursor = decode_cursor::<MediaCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        if cursor.media_type != media_type || cursor.transfer_status != transfer_status {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.type::text AS media_type,
            m.alt_text,
            m.origin_tweet_id,
            m.origin_user_id,
            m.updated_at,
            to_jsonb(resource) AS latest_resource,
            transfer.status::text AS transfer_status,
            transfer.transfer_task_id,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key
        FROM tweet.media AS m
        LEFT JOIN tweet.v_latest_media_resource AS resource
          ON resource.media_id = m.id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = m.id
        WHERE ($1::text = 'all' OR m.type::text = $1)
          AND (
                $2::text = 'all'
            OR ($2 = 'pending' AND transfer.status = 'pending')
            OR ($2 = 'processing' AND transfer.status = 'processing')
            OR ($2 = 'completed' AND transfer.status = 'completed')
            OR ($2 = 'failed' AND transfer.status = 'failed')
            OR ($2 = 'canceled' AND transfer.status = 'canceled')
          )
          AND (
                $3::text IS NULL
            OR m.id::text LIKE $3
            OR m.origin_tweet_id::text LIKE $3
            OR m.origin_user_id::text LIKE $3
          )
          AND (
                NOT $4
            OR (m.updated_at, m.id) < ($5, $6)
          )
        ORDER BY m.updated_at DESC, m.id DESC
        LIMIT $7
        "#,
    )
    .bind(&media_type)
    .bind(&transfer_status)
    .bind(q_prefix.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.updated_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: i64 = row.get("id");
        (
            json!({
                "id": id.to_string(),
                "type": row.get::<String, _>("media_type"),
                "altText": row.get::<Option<String>, _>("alt_text").map(|value| truncate_text(&value, 140)),
                "originTweetId": row.get::<Option<i64>, _>("origin_tweet_id").map(|value| value.to_string()),
                "originUserId": row.get::<Option<i64>, _>("origin_user_id").map(|value| value.to_string()),
                "updatedAt": format_time(updated_at),
                "latestResource": row.get::<Option<Value>, _>("latest_resource"),
                "transferStatus": row.get::<Option<String>, _>("transfer_status"),
                "transferTaskId": row.get::<Option<Uuid>, _>("transfer_task_id"),
                "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
                "storageObjectKey": row.get::<Option<String>, _>("storage_object_key"),
            }),
            MediaCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                media_type: media_type.clone(),
                transfer_status: transfer_status.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            m.id,
            m.type::text AS media_type,
            m.alt_text,
            m.origin_tweet_id,
            m.origin_user_id,
            m.updated_at,
            to_jsonb(m) AS record,
            to_jsonb(resource) AS latest_resource,
            transfer.status::text AS transfer_status,
            transfer.transfer_task_id,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key
        FROM tweet.media AS m
        LEFT JOIN tweet.v_latest_media_resource AS resource
          ON resource.media_id = m.id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = m.id
        WHERE m.id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("media not found"))?;
    let tweets = fetch_media_tweets(&state.db, media_id).await?;
    let tasks = fetch_media_transfer_tasks(&state.db, media_id).await?;

    Ok(Json(detail_response(
        json!({
            "id": media_id.to_string(),
            "type": row.get::<String, _>("media_type"),
            "altText": row.get::<Option<String>, _>("alt_text"),
            "originTweetId": row.get::<Option<i64>, _>("origin_tweet_id").map(|value| value.to_string()),
            "originUserId": row.get::<Option<i64>, _>("origin_user_id").map(|value| value.to_string()),
            "transferStatus": row.get::<Option<String>, _>("transfer_status"),
            "transferTaskId": row.get::<Option<Uuid>, _>("transfer_task_id"),
            "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
            "storageObjectKey": row.get::<Option<String>, _>("storage_object_key"),
            "updatedAt": format_time(row.get("updated_at")),
        }),
        row.get("record"),
        json!({
            "latestResource": row.get::<Option<Value>, _>("latest_resource"),
            "tweets": tweets,
            "transferTasks": tasks,
        }),
    )))
}

pub async fn create_media_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            m.id,
            m.type::text AS media_type,
            resource.recorded_at,
            CASE
                WHEN m.type = 'photo' THEN resource.media_url
                ELSE COALESCE(variant.url, resource.media_url)
            END AS source_url,
            CASE
                WHEN m.type = 'photo' THEN 'media_url'
                WHEN variant.url IS NOT NULL THEN 'video_variant'
                ELSE 'media_url'
            END AS source_kind,
            CASE
                WHEN m.type = 'photo' THEN NULL
                ELSE variant.content_type
            END AS source_content_type
        FROM tweet.media AS m
        INNER JOIN tweet.v_latest_media_resource AS resource
          ON resource.media_id = m.id
        LEFT JOIN LATERAL (
            SELECT
                (item).url AS url,
                dict.value AS content_type,
                (item).bitrate AS bitrate
            FROM unnest(COALESCE((resource.video).variants, ARRAY[]::tweet.video_variant[])) AS item
            LEFT JOIN tweet.string_dict AS dict
              ON dict.id = (item).content_type_id
            ORDER BY
                CASE WHEN dict.value = 'video/mp4' THEN 0 ELSE 1 END,
                (item).bitrate DESC NULLS LAST
            LIMIT 1
        ) AS variant ON TRUE
        WHERE m.id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("media resource not found"))?;
    let source_url: String = row
        .get::<Option<String>, _>("source_url")
        .ok_or_else(|| AppError::bad_request("media has no available transfer source"))?;
    let source_kind: String = row.get("source_kind");
    let source_content_type: Option<String> = row.get("source_content_type");
    let task_id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO media.transfer_task (
            id,
            media_id,
            source_recorded_at,
            source_url,
            source_kind,
            source_content_type,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'pending')
        ON CONFLICT (media_id, source_recorded_at) DO NOTHING
        "#,
    )
    .bind(task_id)
    .bind(media_id)
    .bind(row.get::<OffsetDateTime, _>("recorded_at"))
    .bind(&source_url)
    .bind(&source_kind)
    .bind(source_content_type.as_deref())
    .execute(&mut *tx)
    .await?;

    db::insert_audit_event_tx(
        &mut tx,
        admin.record.user_id,
        "transfer.enqueued",
        "media",
        Some(media_id.to_string()),
        json!({
            "task_id": task_id,
            "inserted": inserted.rows_affected() > 0,
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "created": inserted.rows_affected() > 0,
        "taskId": task_id,
    })))
}

pub async fn list_storage_objects(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<StorageObjectListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let cursor = decode_cursor::<StorageObjectCursor>(query.list.cursor.as_deref())?;

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

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
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
            object.id,
            object.provider,
            object.bucket,
            object.object_key,
            object.content_type,
            object.content_length,
            object.etag,
            object.sha256_hex,
            object.created_at,
            to_jsonb(object) AS record,
            0::bigint AS task_count
        FROM media.storage_object AS object
        WHERE object.id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("storage object not found"))?;
    let transfer_tasks = fetch_storage_object_tasks(&state.db, object_id).await?;

    Ok(Json(detail_response(
        storage_object_json_from_row(&row),
        row.get("record"),
        json!({
            "transferTasks": transfer_tasks,
        }),
    )))
}

pub async fn open_storage_object(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(object_id): Path<Uuid>,
) -> AppResult<Redirect> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT bucket, object_key
        FROM media.storage_object
        WHERE id = $1
        "#,
    )
    .bind(object_id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("storage object not found"))?;
    let client = storage::build_client(&state.settings)?;
    let url = storage::presign_get_object(
        &client,
        &row.get::<String, _>("bucket"),
        &row.get::<String, _>("object_key"),
        Duration::from_secs(300),
    )
    .await?;

    Ok(Redirect::temporary(&url))
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
            COUNT(*) FILTER (WHERE status = 'completed') AS completed,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed,
            COUNT(*) FILTER (WHERE status = 'canceled') AS canceled
        FROM media.transfer_task
        "#,
    )
    .fetch_one(&state.db)
    .await?;
    let recent_failed_tasks = fetch_transfer_task_array(
        &state.db,
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
        WHERE task.status IN ('failed', 'canceled')
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT 20
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
            "completed": counts.get::<i64, _>("completed"),
            "failed": counts.get::<i64, _>("failed"),
            "canceled": counts.get::<i64, _>("canceled"),
        },
        "recentFailedTasks": recent_failed_tasks,
    })))
}

pub async fn list_transfer_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TransferTaskListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
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
    let rows = sqlx::query(
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
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
    )
    .bind(&status)
    .bind(q_prefix.as_deref())
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

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = fetch_transfer_task_row(&state.db, task_id).await?;
    let media = fetch_transfer_task_media(&state.db, task_id).await?;
    let audit_events =
        fetch_audit_events(&state.db, "media_transfer_task", &task_id.to_string()).await?;

    Ok(Json(detail_response(
        transfer_task_json_from_row(&row),
        row.get("record"),
        json!({
            "media": media,
            "auditEvents": audit_events,
        }),
    )))
}

pub async fn retry_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
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
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer task not found"))?;
    let status: String = row.get("status");
    if !matches!(status.as_str(), "failed" | "canceled") {
        return Err(AppError::bad_request(
            "task status must be failed or canceled",
        ));
    }

    sqlx::query(
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
    .bind(task_id)
    .execute(&mut *tx)
    .await?;

    insert_transfer_audit(
        &mut tx,
        admin.record.user_id,
        "transfer.retried",
        task_id,
        &row,
    )
    .await?;
    tx.commit().await?;
    let task = fetch_transfer_task_summary(&state.db, task_id).await?;

    Ok(Json(json!({
        "ok": true,
        "task": task,
    })))
}

pub async fn cancel_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
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
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer task not found"))?;
    let status: String = row.get("status");
    if status != "pending" {
        return Err(AppError::bad_request("task status must be pending"));
    }

    sqlx::query(
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
    .bind(task_id)
    .execute(&mut *tx)
    .await?;

    insert_transfer_audit(
        &mut tx,
        admin.record.user_id,
        "transfer.canceled",
        task_id,
        &row,
    )
    .await?;
    tx.commit().await?;
    let task = fetch_transfer_task_summary(&state.db, task_id).await?;

    Ok(Json(json!({
        "ok": true,
        "task": task,
    })))
}

pub async fn release_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
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
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("transfer task not found"))?;
    let status: String = row.get("status");
    if status != "processing" {
        return Err(AppError::bad_request("task status must be processing"));
    }

    sqlx::query(
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
    .bind(task_id)
    .execute(&mut *tx)
    .await?;

    insert_transfer_audit(
        &mut tx,
        admin.record.user_id,
        "transfer.released",
        task_id,
        &row,
    )
    .await?;
    tx.commit().await?;
    let task = fetch_transfer_task_summary(&state.db, task_id).await?;

    Ok(Json(json!({
        "ok": true,
        "task": task,
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

fn parse_optional_i64(value: Option<&str>, field: &str) -> AppResult<Option<i64>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<i64>().map_err(|_| {
                AppError::bad_request(format!("{field} must be a signed 64-bit integer"))
            })
        })
        .transpose()
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...");
            break;
        }
        output.push(ch);
    }
    output
}

fn storage_object_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "provider": row.get::<String, _>("provider"),
        "bucket": row.get::<String, _>("bucket"),
        "objectKey": row.get::<String, _>("object_key"),
        "contentType": row.get::<String, _>("content_type"),
        "contentLength": row.get::<i64, _>("content_length"),
        "etag": row.get::<Option<String>, _>("etag"),
        "sha256Hex": row.get::<String, _>("sha256_hex"),
        "createdAt": format_time(row.get("created_at")),
        "taskCount": row.try_get::<i64, _>("task_count").unwrap_or_default(),
    })
}

fn transfer_task_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    let status = row.get::<String, _>("status");
    json!({
        "id": row.get::<Uuid, _>("id"),
        "mediaId": row.get::<i64, _>("media_id").to_string(),
        "sourceRecordedAt": format_time(row.get("source_recorded_at")),
        "sourceUrl": row.get::<String, _>("source_url"),
        "sourceKind": row.get::<String, _>("source_kind"),
        "sourceContentType": row.get::<Option<String>, _>("source_content_type"),
        "status": status,
        "attemptCount": row.get::<i32, _>("attempt_count"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "claimedBy": row.get::<Option<String>, _>("claimed_by"),
        "claimedAt": format_time_opt(row.get("claimed_at")),
        "completedAt": format_time_opt(row.get("completed_at")),
        "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
        "storageObjectKey": row.get::<Option<String>, _>("storage_object_key"),
        "createdAt": format_time(row.get("created_at")),
        "updatedAt": format_time(row.get("updated_at")),
        "canRetry": matches!(status.as_str(), "failed" | "canceled"),
        "canCancel": status == "pending",
        "canRelease": status == "processing",
    })
}

async fn fetch_recent_tweets(pool: &PgPool) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            t.id,
            t.published_at,
            t.author_id,
            (t.legacy_text).body AS text_body,
            snapshot.user_name AS author_user_name
        FROM tweet.tweet AS t
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = t.author_id
        ORDER BY t.published_at DESC, t.id DESC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let text_body: String = row.get("text_body");
                json!({
                    "id": row.get::<i64, _>("id").to_string(),
                    "publishedAt": format_time(row.get("published_at")),
                    "authorId": row.get::<i64, _>("author_id").to_string(),
                    "authorUserName": row.get::<Option<String>, _>("author_user_name"),
                    "text": truncate_text(&text_body, 140),
                })
            })
            .collect(),
    ))
}

async fn fetch_user_tweets(pool: &PgPool, user_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT id, published_at, (legacy_text).body AS text_body
        FROM tweet.tweet
        WHERE author_id = $1
        ORDER BY published_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let text_body: String = row.get("text_body");
                json!({
                    "id": row.get::<i64, _>("id").to_string(),
                    "publishedAt": format_time(row.get("published_at")),
                    "text": truncate_text(&text_body, 140),
                })
            })
            .collect(),
    ))
}

async fn fetch_user_media(pool: &PgPool, user_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT id, type::text AS media_type, alt_text, origin_tweet_id, updated_at
        FROM tweet.media
        WHERE origin_user_id = $1
        ORDER BY updated_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter().map(media_summary_json).collect(),
    ))
}

async fn fetch_tweet_media(pool: &PgPool, tweet_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            media.id,
            media.type::text AS media_type,
            media.alt_text,
            media.origin_tweet_id,
            media.updated_at,
            transfer.status::text AS transfer_status,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = media.id
        WHERE ref.tweet_id = $1
        ORDER BY ref.display_order ASC, media.id ASC
        "#,
    )
    .bind(tweet_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter().map(media_summary_json).collect(),
    ))
}

async fn fetch_media_tweets(pool: &PgPool, media_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            t.id,
            t.published_at,
            t.author_id,
            (t.legacy_text).body AS text_body
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.tweet AS t
          ON t.id = ref.tweet_id
        WHERE ref.media_id = $1
        ORDER BY t.published_at DESC, t.id DESC
        LIMIT 20
        "#,
    )
    .bind(media_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let text_body: String = row.get("text_body");
                json!({
                    "id": row.get::<i64, _>("id").to_string(),
                    "publishedAt": format_time(row.get("published_at")),
                    "authorId": row.get::<i64, _>("author_id").to_string(),
                    "displayOrder": row.get::<i16, _>("display_order"),
                    "text": truncate_text(&text_body, 140),
                })
            })
            .collect(),
    ))
}

async fn fetch_media_transfer_tasks(pool: &PgPool, media_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
        WHERE task.media_id = $1
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT 20
        "#,
    )
    .bind(media_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| transfer_task_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_storage_object_tasks(pool: &PgPool, object_id: Uuid) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
        WHERE task.storage_object_id = $1
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT 20
        "#,
    )
    .bind(object_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| transfer_task_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_transfer_task_array(pool: &PgPool, sql: &str) -> AppResult<Value> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| transfer_task_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_transfer_task_row(pool: &PgPool, task_id: Uuid) -> AppResult<sqlx::postgres::PgRow> {
    let row = sqlx::query(
        r#"
        SELECT
            task.id,
            task.media_id,
            task.source_recorded_at,
            task.source_url,
            task.source_kind,
            task.source_content_type,
            task.status::text AS status,
            task.attempt_count,
            task.last_error,
            task.claimed_by,
            task.claimed_at,
            task.completed_at,
            task.storage_object_id,
            object.object_key AS storage_object_key,
            task.created_at,
            task.updated_at,
            to_jsonb(task) AS record
        FROM media.transfer_task AS task
        LEFT JOIN media.storage_object AS object
          ON object.id = task.storage_object_id
        WHERE task.id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| AppError::not_found("transfer task not found"))
}

async fn fetch_transfer_task_summary(pool: &PgPool, task_id: Uuid) -> AppResult<Value> {
    let row = fetch_transfer_task_row(pool, task_id).await?;
    Ok(transfer_task_json_from_row(&row))
}

async fn fetch_transfer_task_media(pool: &PgPool, task_id: Uuid) -> AppResult<Option<Value>> {
    let row = sqlx::query(
        r#"
        SELECT to_jsonb(t) AS media
        FROM (
            SELECT
                media.id,
                media.type::text AS media_type,
                media.alt_text,
                media.origin_tweet_id,
                media.origin_user_id,
                media.updated_at
            FROM tweet.media AS media
            INNER JOIN media.transfer_task AS task
              ON task.media_id = media.id
            WHERE task.id = $1
        ) AS t
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| row.get("media")))
}

async fn fetch_audit_events(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
        FROM audit.audit_events
        WHERE resource_type = $1
          AND resource_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
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
                    "createdAt": format_time(row.get("created_at")),
                })
            })
            .collect(),
    ))
}

async fn insert_transfer_audit(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    event_type: &str,
    task_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> AppResult<()> {
    db::insert_audit_event_tx(
        tx,
        actor_user_id,
        event_type,
        "media_transfer_task",
        Some(task_id.to_string()),
        json!({
            "media_id": row.get::<i64, _>("media_id").to_string(),
            "previous_status": row.get::<String, _>("status"),
        }),
    )
    .await
}

fn media_summary_json(row: sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<i64, _>("id").to_string(),
        "type": row.get::<String, _>("media_type"),
        "altText": row.get::<Option<String>, _>("alt_text").map(|value| truncate_text(&value, 140)),
        "originTweetId": row.get::<Option<i64>, _>("origin_tweet_id").map(|value| value.to_string()),
        "updatedAt": format_time(row.get("updated_at")),
        "transferStatus": row.try_get::<Option<String>, _>("transfer_status").ok().flatten(),
        "storageObjectId": row.try_get::<Option<Uuid>, _>("storage_object_id").ok().flatten(),
        "storageObjectKey": row.try_get::<Option<String>, _>("storage_object_key").ok().flatten(),
        "displayOrder": row.try_get::<i16, _>("display_order").ok(),
    })
}
