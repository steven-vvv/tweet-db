use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    db,
    error::{AppError, AppResult},
    state::AppState,
};

use super::{common::*, rows::*};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaListQuery {
    #[serde(flatten)]
    list: ListQuery,
    media_type: Option<String>,
    transfer_status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaCursor {
    v: u8,
    q: Option<String>,
    media_type: String,
    transfer_status: String,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaResourceCursor {
    v: u8,
    media_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaTweetCursor {
    v: u8,
    media_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    published_at: OffsetDateTime,
    tweet_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaTransferTaskCursor {
    v: u8,
    media_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<MediaListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
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
    let sql = media_select_sql(
        r#"
        WHERE ($1::text = 'all' OR m.type::text = $1)
          AND (
                $2::text = 'all'
            OR transfer.status::text = $2
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
    );
    let rows = sqlx::query(&sql)
        .bind(&media_type)
        .bind(&transfer_status)
        .bind(q_prefix.as_deref())
        .bind(use_cursor)
        .bind(cursor.as_ref().map(|item| item.updated_at))
        .bind(cursor.as_ref().map(|item| item.id))
        .bind(limit_plus_one(limit))
        .fetch_all(&state.db)
        .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let updated_at: OffsetDateTime = row.get("updated_at");
        let id: i64 = row.get("id");
        let mut item = media_json_from_row(&row);
        if let Some(object) = item.as_object_mut() {
            object.insert(
                "latestTransferTaskId".to_owned(),
                row.get::<Option<Uuid>, _>("transfer_task_id")
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "latestTransferStatus".to_owned(),
                row.get::<Option<String>, _>("transfer_status")
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "storageObjectId".to_owned(),
                row.get::<Option<Uuid>, _>("storage_object_id")
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            );
        }
        (
            item,
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

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    let row = fetch_media_row(&state.db, media_id).await?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();

    if includes.contains("latest-resource") {
        included.insert(
            "latestResource".to_owned(),
            fetch_latest_media_resource(&state.db, media_id)
                .await?
                .unwrap_or(Value::Null),
        );
    }
    if includes.contains("transfer-tasks") {
        included.insert(
            "transferTasks".to_owned(),
            fetch_media_transfer_tasks_array(&state.db, media_id, 20).await?,
        );
    }
    if includes.contains("tweets") {
        included.insert(
            "tweets".to_owned(),
            fetch_media_tweets_array(&state.db, media_id).await?,
        );
    }

    Ok(Json(detail_response(media_json_from_row(&row), included)))
}

pub async fn list_media_resources(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    ensure_media_exists(&state.db, media_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<MediaResourceCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.media_id != media_id {
            return Err(AppError::bad_request("cursor does not match media"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT recorded_at, to_jsonb(resource) AS data
        FROM tweet.media_resource AS resource
        WHERE media_id = $1
          AND (NOT $2 OR recorded_at < $3)
        ORDER BY recorded_at DESC
        LIMIT $4
        "#,
    )
    .bind(media_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.recorded_at))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let recorded_at: OffsetDateTime = row.get("recorded_at");
        (
            row.get::<Value, _>("data"),
            MediaResourceCursor {
                v: CURSOR_VERSION,
                media_id,
                recorded_at,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_media_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    ensure_media_exists(&state.db, media_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<MediaTweetCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.media_id != media_id {
            return Err(AppError::bad_request("cursor does not match media"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            t.id,
            t.published_at,
            t.source_id,
            t.author_id,
            t.place_id,
            to_jsonb(t.legacy_text) AS legacy_text,
            t.note_id,
            to_jsonb(t.note_text) AS note_text,
            t.language_id,
            t.conversation_id,
            t.reply_to_tweet_id,
            t.reply_to_user_id,
            t.quote_tweet_id,
            to_jsonb(t.quote_permalink) AS quote_permalink,
            t.repost_id,
            t.created_at,
            t.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.tweet AS t
          ON t.id = ref.tweet_id
        WHERE ref.media_id = $1
          AND (
                NOT $2
            OR (t.published_at, t.id) < ($3, $4)
          )
        ORDER BY t.published_at DESC, t.id DESC
        LIMIT $5
        "#,
    )
    .bind(media_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.published_at))
    .bind(cursor.as_ref().map(|item| item.tweet_id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let published_at: OffsetDateTime = row.get("published_at");
        let tweet_id: i64 = row.get("id");
        let mut item = tweet_json_from_row(&row);
        if let Some(object) = item.as_object_mut() {
            object.insert(
                "displayOrder".to_owned(),
                json!(row.get::<i16, _>("display_order")),
            );
        }
        (
            item,
            MediaTweetCursor {
                v: CURSOR_VERSION,
                media_id,
                published_at,
                tweet_id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_media_transfer_tasks(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::MediaRead)?;
    ensure_media_exists(&state.db, media_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<MediaTransferTaskCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.media_id != media_id {
            return Err(AppError::bad_request("cursor does not match media"));
        }
    }

    let use_cursor = cursor.is_some();
    let sql = transfer_task_select_sql(
        r#"
        WHERE task.media_id = $1
          AND (
                NOT $2
            OR (task.updated_at, task.id) < ($3, $4)
          )
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT $5
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(media_id)
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
            MediaTransferTaskCursor {
                v: CURSOR_VERSION,
                media_id,
                updated_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn create_media_transfer_task(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(media_id): Path<i64>,
) -> AppResult<Json<ActionResponse>> {
    let admin = require_capability(session, Capability::MediaTransferWrite)?;
    let row = sqlx::query(
        r#"
        SELECT
            m.id,
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
    .await?
    .ok_or_else(|| AppError::not_found("media resource not found"))?;

    let source_url: String = row
        .get::<Option<String>, _>("source_url")
        .ok_or_else(|| AppError::bad_request("media has no available transfer source"))?;
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
    .bind(row.get::<String, _>("source_kind"))
    .bind(row.get::<Option<String>, _>("source_content_type"))
    .execute(&mut *tx)
    .await?;

    db::insert_audit_event_tx(
        &mut tx,
        admin.record.user_id,
        "transfer.enqueued",
        "media",
        Some(media_id.to_string()),
        json!({
            "taskId": task_id,
            "inserted": inserted.rows_affected() > 0,
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(action_response(
        json!({
            "id": task_id,
            "mediaId": json_i64(media_id),
        }),
        json!({
            "ok": true,
            "created": inserted.rows_affected() > 0,
        }),
    )))
}

async fn fetch_media_row(pool: &sqlx::PgPool, media_id: i64) -> AppResult<sqlx::postgres::PgRow> {
    let sql = media_select_sql("WHERE m.id = $1");
    sqlx::query(&sql)
        .bind(media_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("media not found"))
}

async fn ensure_media_exists(pool: &sqlx::PgPool, media_id: i64) -> AppResult<()> {
    fetch_media_row(pool, media_id).await.map(|_| ())
}

async fn fetch_latest_media_resource(
    pool: &sqlx::PgPool,
    media_id: i64,
) -> AppResult<Option<Value>> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT to_jsonb(resource)
        FROM tweet.v_latest_media_resource AS resource
        WHERE media_id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(pool)
    .await?)
}

async fn fetch_media_transfer_tasks_array(
    pool: &sqlx::PgPool,
    media_id: i64,
    limit: i64,
) -> AppResult<Value> {
    let sql = transfer_task_select_sql(
        r#"
        WHERE task.media_id = $1
        ORDER BY task.updated_at DESC, task.id DESC
        LIMIT $2
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(media_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| transfer_task_json_from_row(&row))
            .collect(),
    ))
}

async fn fetch_media_tweets_array(pool: &sqlx::PgPool, media_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            t.id,
            t.published_at,
            t.author_id
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.tweet AS t
          ON t.id = ref.tweet_id
        WHERE ref.media_id = $1
        ORDER BY t.published_at DESC, t.id DESC
        LIMIT 100
        "#,
    )
    .bind(media_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": json_i64(row.get::<i64, _>("id")),
                    "publishedAt": row_time(&row, "published_at"),
                    "authorId": json_i64(row.get::<i64, _>("author_id")),
                    "displayOrder": row.get::<i16, _>("display_order"),
                })
            })
            .collect(),
    ))
}

pub(super) fn media_select_sql(tail: &str) -> String {
    format!(
        r#"
        SELECT
            m.id,
            m.type::text AS media_type,
            m.alt_text,
            m.grok_post_id,
            to_jsonb(m.geometry) AS geometry,
            to_jsonb(m.size_variants) AS size_variants,
            to_jsonb(m.tagged_users) AS tagged_users,
            to_jsonb(m.sensitivity_warning_ids) AS sensitivity_warning_ids,
            m.origin_tweet_id,
            m.origin_user_id,
            to_jsonb(m.details) AS details,
            m.created_at,
            m.updated_at,
            transfer.status::text AS transfer_status,
            transfer.transfer_task_id,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key
        FROM tweet.media AS m
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = m.id
        {tail}
        "#
    )
}

pub(super) fn transfer_task_select_sql(tail: &str) -> String {
    format!(
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
        {tail}
        "#
    )
}
