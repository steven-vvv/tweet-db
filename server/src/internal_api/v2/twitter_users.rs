use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::Row;
use time::OffsetDateTime;
use tokio::try_join;

use crate::{
    auth::ActiveSession,
    error::{AppError, AppResult},
    state::AppState,
};

use super::{common::*, rows::twitter_user_json_from_row};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TwitterUserListQuery {
    #[serde(flatten)]
    list: ListQuery,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TwitterUserSearchQuery {
    #[serde(flatten)]
    list: ListQuery,
    user_ids: Option<String>,
    user_name_prefix: Option<String>,
    display_name_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TwitterUserCursor {
    v: u8,
    q: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TwitterUserChildCursor {
    v: u8,
    user_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
struct TwitterUserSearchCursor {
    v: u8,
    user_ids: Vec<i64>,
    user_name_prefix: Option<String>,
    display_name_prefix: Option<String>,
    offset: usize,
}

pub async fn list_twitter_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TwitterUserListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
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
            to_jsonb(snapshot) AS latest_snapshot,
            to_jsonb(stats) AS latest_stats
        FROM tweet.twitter_user AS u
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = u.id
        LEFT JOIN tweet.v_latest_user_stats AS stats
          ON stats.user_id = u.id
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

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: i64 = row.get("id");
        let mut item = twitter_user_json_from_row(&row);
        if let Some(object) = item.as_object_mut() {
            object.insert(
                "latestSnapshot".to_owned(),
                row.get::<Option<Value>, _>("latest_snapshot")
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "latestStats".to_owned(),
                row.get::<Option<Value>, _>("latest_stats")
                    .unwrap_or(Value::Null),
            );
        }
        (
            item,
            TwitterUserCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_twitter_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let row = fetch_twitter_user_row(&state.db, user_id).await?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();
    let include_snapshot = includes.contains("latest-snapshot");
    let include_stats = includes.contains("latest-stats");
    let (latest_snapshot, latest_stats) = try_join!(
        maybe_fetch(include_snapshot, fetch_latest_snapshot(&state.db, user_id)),
        maybe_fetch(include_stats, fetch_latest_stats(&state.db, user_id)),
    )?;

    if include_snapshot {
        included.insert(
            "latestSnapshot".to_owned(),
            latest_snapshot.flatten().unwrap_or(Value::Null),
        );
    }
    if include_stats {
        included.insert(
            "latestStats".to_owned(),
            latest_stats.flatten().unwrap_or(Value::Null),
        );
    }

    Ok(Json(detail_response(
        twitter_user_json_from_row(&row),
        included,
    )))
}

pub async fn search_twitter_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TwitterUserSearchQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let limit = resolve_limit(query.list.limit);
    let user_ids = parse_i64_csv(query.user_ids.as_deref(), "userIds")?;
    let user_name_prefix = normalize_user_query_prefix(query.user_name_prefix.as_deref());
    let display_name_prefix = normalize_user_query_prefix(query.display_name_prefix.as_deref());
    if user_ids.is_empty() && user_name_prefix.is_none() && display_name_prefix.is_none() {
        return Err(AppError::bad_request(
            "one of userIds, userNamePrefix, displayNamePrefix is required",
        ));
    }

    let cursor = decode_cursor::<TwitterUserSearchCursor>(query.list.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.user_ids != user_ids
            || cursor.user_name_prefix != user_name_prefix
            || cursor.display_name_prefix != display_name_prefix
        {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let offset = cursor.as_ref().map(|item| item.offset).unwrap_or_default();
    let rows = sqlx::query(
        r#"
        WITH candidates AS (
            SELECT unnest($1::BIGINT[]) AS user_id, 1 AS rank
            UNION ALL
            SELECT DISTINCT snapshot.user_id, 2 AS rank
            FROM tweet.user_snapshot AS snapshot
            WHERE $2::TEXT IS NOT NULL
              AND lower(snapshot.user_name) LIKE $2
            UNION ALL
            SELECT DISTINCT snapshot.user_id, 3 AS rank
            FROM tweet.user_snapshot AS snapshot
            WHERE $3::TEXT IS NOT NULL
              AND lower(snapshot.display_name) LIKE $3
        ),
        ranked AS (
            SELECT user_id, MIN(rank) AS rank
            FROM candidates
            GROUP BY user_id
        )
        SELECT
            ranked.rank,
            u.id,
            u.registered_at,
            u.created_at,
            u.updated_at,
            to_jsonb(snapshot) AS latest_snapshot,
            to_jsonb(stats) AS latest_stats
        FROM ranked
        INNER JOIN tweet.twitter_user AS u
          ON u.id = ranked.user_id
        LEFT JOIN tweet.v_latest_user_snapshot AS snapshot
          ON snapshot.user_id = u.id
        LEFT JOIN tweet.v_latest_user_stats AS stats
          ON stats.user_id = u.id
        ORDER BY ranked.rank ASC, u.updated_at DESC, u.id DESC
        OFFSET $4
        LIMIT $5
        "#,
    )
    .bind(&user_ids)
    .bind(user_name_prefix.as_deref().map(|value| format!("{value}%")))
    .bind(display_name_prefix.as_deref().map(|value| format!("{value}%")))
    .bind(offset as i64)
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let has_more = rows.len() > limit;
    let data = rows
        .into_iter()
        .take(limit)
        .map(twitter_user_summary_json_from_row)
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| {
        encode_cursor(&TwitterUserSearchCursor {
            v: CURSOR_VERSION,
            user_ids,
            user_name_prefix,
            display_name_prefix,
            offset: offset.saturating_add(limit),
        })
    }).transpose()?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_twitter_user_snapshots(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    ensure_twitter_user_exists(&state.db, user_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<TwitterUserChildCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.user_id != user_id {
            return Err(AppError::bad_request("cursor does not match user"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT recorded_at, to_jsonb(snapshot) AS data
        FROM tweet.user_snapshot AS snapshot
        WHERE user_id = $1
          AND (NOT $2 OR recorded_at < $3)
        ORDER BY recorded_at DESC
        LIMIT $4
        "#,
    )
    .bind(user_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.recorded_at))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let recorded_at: OffsetDateTime = row.get("recorded_at");
        (
            row.get::<Value, _>("data"),
            TwitterUserChildCursor {
                v: CURSOR_VERSION,
                user_id,
                recorded_at,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_twitter_user_stats(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    ensure_twitter_user_exists(&state.db, user_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<TwitterUserChildCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.user_id != user_id {
            return Err(AppError::bad_request("cursor does not match user"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT recorded_at, to_jsonb(stats) AS data
        FROM tweet.user_stats AS stats
        WHERE user_id = $1
          AND (NOT $2 OR recorded_at < $3)
        ORDER BY recorded_at DESC
        LIMIT $4
        "#,
    )
    .bind(user_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.recorded_at))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let recorded_at: OffsetDateTime = row.get("recorded_at");
        (
            row.get::<Value, _>("data"),
            TwitterUserChildCursor {
                v: CURSOR_VERSION,
                user_id,
                recorded_at,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

async fn fetch_twitter_user_row(
    pool: &sqlx::PgPool,
    user_id: i64,
) -> AppResult<sqlx::postgres::PgRow> {
    sqlx::query(
        r#"
        SELECT id, registered_at, created_at, updated_at
        FROM tweet.twitter_user
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("twitter user not found"))
}

fn twitter_user_summary_json_from_row(row: sqlx::postgres::PgRow) -> Value {
    let mut item = twitter_user_json_from_row(&row);
    if let Some(object) = item.as_object_mut() {
        object.insert(
            "latestSnapshot".to_owned(),
            row.get::<Option<Value>, _>("latest_snapshot")
                .unwrap_or(Value::Null),
        );
        object.insert(
            "latestStats".to_owned(),
            row.get::<Option<Value>, _>("latest_stats")
                .unwrap_or(Value::Null),
        );
    }
    item
}

fn normalize_user_query_prefix(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_start_matches('@').trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

async fn ensure_twitter_user_exists(pool: &sqlx::PgPool, user_id: i64) -> AppResult<()> {
    fetch_twitter_user_row(pool, user_id).await.map(|_| ())
}

async fn fetch_latest_snapshot(pool: &sqlx::PgPool, user_id: i64) -> AppResult<Option<Value>> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT to_jsonb(snapshot)
        FROM tweet.v_latest_user_snapshot AS snapshot
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_user_query_prefixes() {
        assert_eq!(
            normalize_user_query_prefix(Some(" @Alice ")).as_deref(),
            Some("alice")
        );
        assert_eq!(normalize_user_query_prefix(Some("   ")), None);
    }
}

async fn fetch_latest_stats(pool: &sqlx::PgPool, user_id: i64) -> AppResult<Option<Value>> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT to_jsonb(stats)
        FROM tweet.v_latest_user_stats AS stats
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}
