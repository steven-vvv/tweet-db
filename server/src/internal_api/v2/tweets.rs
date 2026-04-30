use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;
use time::OffsetDateTime;

use crate::{
    auth::ActiveSession,
    error::{AppError, AppResult},
    state::AppState,
};

use super::{common::*, rows::*};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TweetListQuery {
    #[serde(flatten)]
    list: ListQuery,
    author_id: Option<String>,
    relation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetCursor {
    v: u8,
    q: Option<String>,
    author_id: Option<i64>,
    relation: String,
    #[serde(with = "time::serde::rfc3339")]
    published_at: OffsetDateTime,
    id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetMediaCursor {
    v: u8,
    tweet_id: i64,
    display_order: i16,
    media_id: i64,
}

pub async fn list_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TweetListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let author_id = parse_optional_i64(query.author_id.as_deref(), "authorId")?;
    let relation = normalize_tweet_relation(query.relation.as_deref())?;
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
    let sql = tweet_select_sql(
        r#"
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
    );
    let rows = sqlx::query(&sql)
        .bind(author_id)
        .bind(&relation)
        .bind(q_prefix.as_deref())
        .bind(use_cursor)
        .bind(cursor.as_ref().map(|item| item.published_at))
        .bind(cursor.as_ref().map(|item| item.id))
        .bind(limit_plus_one(limit))
        .fetch_all(&state.db)
        .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let published_at: OffsetDateTime = row.get("published_at");
        let id: i64 = row.get("id");
        (
            tweet_json_from_row(&row),
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

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn get_tweet(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(tweet_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let row = fetch_tweet_row(&state.db, tweet_id).await?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();

    if includes.contains("stats") {
        included.insert(
            "latestStats".to_owned(),
            fetch_tweet_optional(
                &state.db,
                "tweet.v_latest_tweet_stats",
                "tweet_id",
                tweet_id,
            )
            .await?
            .unwrap_or(Value::Null),
        );
    }
    if includes.contains("edit") {
        included.insert(
            "edit".to_owned(),
            fetch_tweet_optional(&state.db, "tweet.tweet_edit", "tweet_id", tweet_id)
                .await?
                .unwrap_or(Value::Null),
        );
    }
    if includes.contains("policy") {
        included.insert(
            "policy".to_owned(),
            fetch_tweet_optional(&state.db, "tweet.tweet_policy", "tweet_id", tweet_id)
                .await?
                .unwrap_or(Value::Null),
        );
    }
    if includes.contains("community-note") {
        included.insert(
            "communityNote".to_owned(),
            fetch_tweet_optional(
                &state.db,
                "tweet.tweet_community_note",
                "tweet_id",
                tweet_id,
            )
            .await?
            .unwrap_or(Value::Null),
        );
    }
    if includes.contains("media") {
        included.insert(
            "media".to_owned(),
            fetch_tweet_media_array(&state.db, tweet_id).await?,
        );
    }

    Ok(Json(detail_response(tweet_json_from_row(&row), included)))
}

pub async fn list_tweet_media(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(tweet_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    ensure_tweet_exists(&state.db, tweet_id).await?;
    let limit = resolve_limit(query.limit);
    let cursor = decode_cursor::<TweetMediaCursor>(query.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        if cursor.tweet_id != tweet_id {
            return Err(AppError::bad_request("cursor does not match tweet"));
        }
    }

    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            media.id,
            media.type::text AS media_type,
            media.alt_text,
            media.grok_post_id,
            to_jsonb(media.geometry) AS geometry,
            to_jsonb(media.size_variants) AS size_variants,
            to_jsonb(media.tagged_users) AS tagged_users,
            to_jsonb(media.sensitivity_warning_ids) AS sensitivity_warning_ids,
            media.origin_tweet_id,
            media.origin_user_id,
            to_jsonb(media.details) AS details,
            media.created_at,
            media.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        WHERE ref.tweet_id = $1
          AND (
                NOT $2
            OR (ref.display_order, media.id) > ($3, $4)
          )
        ORDER BY ref.display_order ASC, media.id ASC
        LIMIT $5
        "#,
    )
    .bind(tweet_id)
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.display_order))
    .bind(cursor.as_ref().map(|item| item.media_id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (data, next_cursor) = paginate_rows(rows, limit, |row| {
        let display_order: i16 = row.get("display_order");
        let media_id: i64 = row.get("id");
        let mut item = media_json_from_row(&row);
        if let Some(object) = item.as_object_mut() {
            object.insert("displayOrder".to_owned(), json!(display_order));
        }
        (
            item,
            TweetMediaCursor {
                v: CURSOR_VERSION,
                tweet_id,
                display_order,
                media_id,
            },
        )
    })?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

async fn fetch_tweet_row(pool: &sqlx::PgPool, tweet_id: i64) -> AppResult<sqlx::postgres::PgRow> {
    let sql = tweet_select_sql("WHERE t.id = $1");
    sqlx::query(&sql)
        .bind(tweet_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("tweet not found"))
}

async fn ensure_tweet_exists(pool: &sqlx::PgPool, tweet_id: i64) -> AppResult<()> {
    fetch_tweet_row(pool, tweet_id).await.map(|_| ())
}

async fn fetch_tweet_optional(
    pool: &sqlx::PgPool,
    table: &str,
    column: &str,
    tweet_id: i64,
) -> AppResult<Option<Value>> {
    let sql = format!("SELECT to_jsonb(item) FROM {table} AS item WHERE {column} = $1");
    Ok(sqlx::query_scalar(&sql)
        .bind(tweet_id)
        .fetch_optional(pool)
        .await?)
}

async fn fetch_tweet_media_array(pool: &sqlx::PgPool, tweet_id: i64) -> AppResult<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            ref.display_order,
            media.id,
            media.type::text AS media_type,
            media.alt_text,
            media.grok_post_id,
            to_jsonb(media.geometry) AS geometry,
            to_jsonb(media.size_variants) AS size_variants,
            to_jsonb(media.tagged_users) AS tagged_users,
            to_jsonb(media.sensitivity_warning_ids) AS sensitivity_warning_ids,
            media.origin_tweet_id,
            media.origin_user_id,
            to_jsonb(media.details) AS details,
            media.created_at,
            media.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        WHERE ref.tweet_id = $1
        ORDER BY ref.display_order ASC, media.id ASC
        LIMIT 100
        "#,
    )
    .bind(tweet_id)
    .fetch_all(pool)
    .await?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let mut item = media_json_from_row(&row);
                if let Some(object) = item.as_object_mut() {
                    object.insert(
                        "displayOrder".to_owned(),
                        json!(row.get::<i16, _>("display_order")),
                    );
                }
                item
            })
            .collect(),
    ))
}

fn tweet_select_sql(tail: &str) -> String {
    format!(
        r#"
        SELECT
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
        FROM tweet.tweet AS t
        {tail}
        "#
    )
}
