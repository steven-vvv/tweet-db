use std::collections::HashMap;

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::ActiveSession,
    error::{AppError, AppResult},
    search::{SearchHit, TweetSearchFilters, TweetSearchSort, UserSearchFilters, UserSearchSort},
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
    sort: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TweetSearchListQuery {
    #[serde(flatten)]
    list: ListQuery,
    tweet_ids: Option<String>,
    author_ids: Option<String>,
    author_user_names: Option<String>,
    author_id: Option<String>,
    relation: Option<String>,
    sort: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserTweetListQuery {
    #[serde(flatten)]
    list: ListQuery,
    relation: Option<String>,
    sort: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TweetListSort {
    PublishedAt,
    CreatedAt,
    UpdatedAt,
}

impl Default for TweetListSort {
    fn default() -> Self {
        Self::PublishedAt
    }
}

impl TweetListSort {
    fn parse(raw: Option<&str>) -> AppResult<Self> {
        match raw.unwrap_or("publishedAt").trim() {
            "publishedAt" => Ok(Self::PublishedAt),
            "createdAt" => Ok(Self::CreatedAt),
            "updatedAt" => Ok(Self::UpdatedAt),
            _ => Err(AppError::bad_request(
                "sort must be one of publishedAt, createdAt, updatedAt",
            )),
        }
    }

    fn sql_column(self) -> &'static str {
        match self {
            Self::PublishedAt => "published_at",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
        }
    }
}

impl From<TweetSearchSort> for TweetListSort {
    fn from(value: TweetSearchSort) -> Self {
        match value {
            TweetSearchSort::Relevance | TweetSearchSort::PublishedAt => Self::PublishedAt,
            TweetSearchSort::CreatedAt => Self::CreatedAt,
            TweetSearchSort::UpdatedAt => Self::UpdatedAt,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetCursor {
    v: u8,
    q: Option<String>,
    author_id: Option<i64>,
    relation: String,
    #[serde(default)]
    sort: TweetListSort,
    #[serde(default, with = "time::serde::rfc3339::option")]
    sort_at: Option<OffsetDateTime>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    published_at: Option<OffsetDateTime>,
    id: i64,
}

impl TweetCursor {
    fn sort_time(&self) -> AppResult<OffsetDateTime> {
        self.sort_at
            .or(self.published_at)
            .ok_or_else(|| AppError::bad_request("invalid cursor"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetMediaCursor {
    v: u8,
    tweet_id: i64,
    display_order: i16,
    media_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TweetSearchCursor {
    v: u8,
    q: Option<String>,
    tweet_ids: Vec<i64>,
    author_ids: Vec<i64>,
    author_user_names: Vec<String>,
    relation: String,
    sort: TweetSearchSort,
    offset: usize,
}

#[derive(Debug)]
struct TweetQuerySpec {
    q: Option<String>,
    tweet_ids: Vec<i64>,
    author_ids: Vec<i64>,
    author_user_names: Vec<String>,
    relation: String,
    sort: TweetSearchSort,
    limit: usize,
    cursor: Option<String>,
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
    let sort = TweetListSort::parse(query.sort.as_deref())?;
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
        if cursor.sort != sort {
            return Err(AppError::bad_request("cursor does not match sort"));
        }
    }

    let q_prefix = prefix_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let sort_column = sort.sql_column();
    let cursor_sort_at = cursor.as_ref().map(TweetCursor::sort_time).transpose()?;
    let tail = format!(
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
            OR (t.{sort_column}, t.id) < ($5, $6)
          )
        ORDER BY t.{sort_column} DESC, t.id DESC
        LIMIT $7
        "#
    );
    let sql = tweet_select_sql(&tail);
    let rows = sqlx::query(&sql)
        .bind(author_id)
        .bind(&relation)
        .bind(q_prefix.as_deref())
        .bind(use_cursor)
        .bind(cursor_sort_at)
        .bind(cursor.as_ref().map(|item| item.id))
        .bind(limit_plus_one(limit))
        .fetch_all(&state.db)
        .await?;

    let (mut data, next_cursor) = paginate_rows(rows, limit, |row| {
        let sort_at: OffsetDateTime = row.get(sort_column);
        let id: i64 = row.get("id");
        (
            tweet_json_from_row(&row),
            TweetCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                author_id,
                relation: relation.clone(),
                sort,
                sort_at: Some(sort_at),
                published_at: None,
                id,
            },
        )
    })?;
    hydrate_browse_tweet_list(&state.db, &mut data).await?;

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
    let author_id: i64 = row.get("author_id");

    let include_stats = includes.contains("stats");
    let include_edit = includes.contains("edit");
    let include_policy = includes.contains("policy");
    let include_note = includes.contains("community-note");
    let include_author = includes.contains("author");
    let include_media = includes.contains("media");
    let (latest_stats, edit, policy, community_note, author, media) = tokio::try_join!(
        maybe_fetch(
            include_stats,
            fetch_tweet_optional(
                &state.db,
                "tweet.v_latest_tweet_stats",
                "tweet_id",
                tweet_id
            )
        ),
        maybe_fetch(
            include_edit,
            fetch_tweet_optional(&state.db, "tweet.tweet_edit", "tweet_id", tweet_id)
        ),
        maybe_fetch(
            include_policy,
            fetch_tweet_optional(&state.db, "tweet.tweet_policy", "tweet_id", tweet_id)
        ),
        maybe_fetch(
            include_note,
            fetch_tweet_optional(
                &state.db,
                "tweet.tweet_community_note",
                "tweet_id",
                tweet_id
            )
        ),
        maybe_fetch(include_author, fetch_author_summary(&state.db, author_id)),
        maybe_fetch(
            include_media,
            fetch_tweet_media_array(&state.db, tweet_id, includes.contains("media-resources"))
        ),
    )?;

    if include_stats {
        included.insert(
            "latestStats".to_owned(),
            latest_stats.flatten().unwrap_or(Value::Null),
        );
    }
    if include_edit {
        included.insert("edit".to_owned(), edit.flatten().unwrap_or(Value::Null));
    }
    if include_policy {
        included.insert("policy".to_owned(), policy.flatten().unwrap_or(Value::Null));
    }
    if include_note {
        included.insert(
            "communityNote".to_owned(),
            community_note.flatten().unwrap_or(Value::Null),
        );
    }
    if include_author {
        included.insert("author".to_owned(), author.flatten().unwrap_or(Value::Null));
    }
    if include_media {
        included.insert("media".to_owned(), media.unwrap_or_else(|| json!([])));
    }

    Ok(Json(detail_response(tweet_json_from_row(&row), included)))
}

pub async fn search_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<TweetSearchListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let spec = tweet_query_spec_from_search_query(query)?;
    let limit = spec.limit;
    let (data, next_cursor) = execute_tweet_query(&state, spec).await?;

    Ok(Json(list_response(data, limit, next_cursor)))
}

pub async fn list_twitter_user_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<i64>,
    Query(query): Query<UserTweetListQuery>,
) -> AppResult<Json<ListResponse>> {
    let _session = require_capability(session, Capability::TweetRead)?;
    let limit = resolve_limit(query.list.limit);
    let spec = TweetQuerySpec {
        q: normalize_query(query.list.q.as_deref()),
        tweet_ids: Vec::new(),
        author_ids: vec![user_id],
        author_user_names: Vec::new(),
        relation: normalize_tweet_relation(query.relation.as_deref())?,
        sort: match query.sort.as_deref() {
            Some(value) => TweetSearchSort::parse(Some(value))?,
            None if normalize_query(query.list.q.as_deref()).is_some() => {
                TweetSearchSort::Relevance
            }
            None => TweetSearchSort::PublishedAt,
        },
        limit,
        cursor: query.list.cursor,
    };
    let (data, next_cursor) = execute_tweet_query(&state, spec).await?;

    Ok(Json(list_response(data, limit, next_cursor)))
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
            transfer.status::text AS transfer_status,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key,
            media.created_at,
            media.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = media.id
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
            insert_media_transfer_fields(object, &row);
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

fn tweet_query_spec_from_search_query(query: TweetSearchListQuery) -> AppResult<TweetQuerySpec> {
    let mut author_ids = parse_i64_csv(query.author_ids.as_deref(), "authorIds")?;
    if let Some(author_id) = parse_optional_i64(query.author_id.as_deref(), "authorId")? {
        author_ids.push(author_id);
        author_ids.sort_unstable();
        author_ids.dedup();
    }

    let q = normalize_query(query.list.q.as_deref());
    let sort = match query.sort.as_deref() {
        Some(value) => TweetSearchSort::parse(Some(value))?,
        None if q.is_some() => TweetSearchSort::Relevance,
        None => TweetSearchSort::PublishedAt,
    };

    Ok(TweetQuerySpec {
        q,
        tweet_ids: parse_i64_csv(query.tweet_ids.as_deref(), "tweetIds")?,
        author_ids,
        author_user_names: normalize_user_name_csv(query.author_user_names.as_deref()),
        relation: normalize_tweet_relation(query.relation.as_deref())?,
        sort,
        limit: resolve_limit(query.list.limit),
        cursor: query.list.cursor,
    })
}

async fn execute_tweet_query(
    state: &AppState,
    mut spec: TweetQuerySpec,
) -> AppResult<(Vec<Value>, Option<String>)> {
    let cursor = decode_cursor::<TweetSearchCursor>(spec.cursor.as_deref())?;
    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            spec.q.as_deref(),
            "cursor does not match query",
        )?;
        if cursor.tweet_ids != spec.tweet_ids
            || cursor.author_user_names != spec.author_user_names
            || cursor.relation != spec.relation
            || cursor.sort != spec.sort
        {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    let search = state
        .search
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("search subsystem is disabled"))?;
    let mut resolved_author_ids =
        resolve_author_user_name_ids(search, &spec.author_user_names).await?;
    spec.author_ids.append(&mut resolved_author_ids);
    spec.author_ids.sort_unstable();
    spec.author_ids.dedup();
    if let Some(cursor) = cursor.as_ref() {
        if cursor.author_ids != spec.author_ids {
            return Err(AppError::bad_request("cursor does not match filters"));
        }
    }

    execute_tweet_search_query(state, search, spec, cursor).await
}

async fn execute_tweet_search_query(
    state: &AppState,
    search: &crate::search::SearchState,
    spec: TweetQuerySpec,
    cursor: Option<TweetSearchCursor>,
) -> AppResult<(Vec<Value>, Option<String>)> {
    let offset = cursor.as_ref().map(|item| item.offset).unwrap_or_default();
    let filters = TweetSearchFilters {
        tweet_ids: spec.tweet_ids.clone(),
        author_ids: spec.author_ids.clone(),
        author_id: None,
        relation: Some(spec.relation.clone()),
    };
    let mut hits = search
        .search_tweets(
            spec.q.as_deref(),
            &filters,
            spec.sort,
            spec.limit.saturating_add(1),
            offset,
        )
        .await?;
    let has_more = hits.len() > spec.limit;
    if has_more {
        hits.truncate(spec.limit);
    }

    let mut data = fetch_tweets_by_search_hits(&state.db, &hits).await?;
    hydrate_browse_tweet_list(&state.db, &mut data).await?;
    let next_cursor = has_more
        .then(|| {
            encode_cursor(&TweetSearchCursor {
                v: CURSOR_VERSION,
                q: spec.q.clone(),
                tweet_ids: spec.tweet_ids.clone(),
                author_ids: spec.author_ids.clone(),
                author_user_names: spec.author_user_names.clone(),
                relation: spec.relation.clone(),
                sort: spec.sort,
                offset: offset.saturating_add(spec.limit),
            })
        })
        .transpose()?;

    Ok((data, next_cursor))
}

async fn resolve_author_user_name_ids(
    search: &crate::search::SearchState,
    user_names: &[String],
) -> AppResult<Vec<i64>> {
    if user_names.is_empty() {
        return Ok(Vec::new());
    }

    let hits = search
        .search_users(
            &UserSearchFilters {
                user_names: user_names.to_vec(),
                ..Default::default()
            },
            UserSearchSort::Relevance,
            user_names.len().saturating_mul(4).max(1),
            0,
        )
        .await?;

    Ok(hits
        .into_iter()
        .map(|hit| hit.id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect())
}

async fn fetch_tweets_by_search_hits(pool: &PgPool, hits: &[SearchHit]) -> AppResult<Vec<Value>> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let ids = hits.iter().map(|hit| hit.id).collect::<Vec<_>>();
    let sql = format!(
        r#"
        SELECT
            input.ord,
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
        FROM unnest($1::BIGINT[]) WITH ORDINALITY AS input(id, ord)
        INNER JOIN tweet.tweet AS t
          ON t.id = input.id
        ORDER BY input.ord
        "#
    );
    let rows = sqlx::query(&sql).bind(&ids).fetch_all(pool).await?;
    let by_id = rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.get("id");
            (id, tweet_json_from_row(&row))
        })
        .collect::<HashMap<_, _>>();

    Ok(hits
        .iter()
        .filter_map(|hit| {
            by_id.get(&hit.id).cloned().map(|mut item| {
                if let Some(object) = item.as_object_mut() {
                    object.insert("searchScore".to_owned(), json!(hit.score));
                    object.insert("searchSortTime".to_owned(), json!(hit.sort_time));
                }
                item
            })
        })
        .collect())
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

async fn fetch_tweet_media_array(
    pool: &sqlx::PgPool,
    tweet_id: i64,
    include_latest_resource: bool,
) -> AppResult<Value> {
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
            to_jsonb(resource) AS latest_resource,
            warnings.sensitivity_warnings,
            transfer.status::text AS transfer_status,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key,
            media.created_at,
            media.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        LEFT JOIN tweet.v_latest_media_resource AS resource
          ON resource.media_id = media.id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = media.id
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(dict.value ORDER BY warning.ord) FILTER (WHERE dict.value IS NOT NULL) AS sensitivity_warnings
            FROM unnest(media.sensitivity_warning_ids) WITH ORDINALITY AS warning(id, ord)
            LEFT JOIN tweet.string_dict AS dict
              ON dict.id = warning.id
        ) AS warnings ON TRUE
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
                    object.insert(
                        "sensitivityWarnings".to_owned(),
                        row.get::<Option<Value>, _>("sensitivity_warnings")
                            .unwrap_or_else(|| json!([])),
                    );
                    insert_media_transfer_fields(object, &row);
                    if include_latest_resource {
                        object.insert(
                            "latestResource".to_owned(),
                            row.get::<Option<Value>, _>("latest_resource")
                                .unwrap_or(Value::Null),
                        );
                    }
                }
                item
            })
            .collect(),
    ))
}

async fn hydrate_tweet_list(
    pool: &sqlx::PgPool,
    data: &mut [Value],
    includes: &IncludeSet,
) -> AppResult<()> {
    if data.is_empty() {
        return Ok(());
    }

    let include_author = includes.contains("author");
    let include_stats = includes.contains("stats");
    let include_media = includes.contains("media") || includes.contains("media-resources");
    if !include_author && !include_stats && !include_media {
        return Ok(());
    }

    let tweet_ids = data
        .iter()
        .filter_map(|item| string_i64_field(item, "id"))
        .collect::<Vec<_>>();
    let author_ids = data
        .iter()
        .filter_map(|item| string_i64_field(item, "authorId"))
        .collect::<Vec<_>>();

    let (authors, stats, media) = tokio::try_join!(
        maybe_fetch(include_author, fetch_author_summary_map(pool, &author_ids)),
        maybe_fetch(include_stats, fetch_tweet_stats_map(pool, &tweet_ids)),
        maybe_fetch(
            include_media,
            fetch_tweet_media_map(pool, &tweet_ids, includes.contains("media-resources"))
        ),
    )?;
    let authors = authors.unwrap_or_default();
    let stats = stats.unwrap_or_default();
    let media = media.unwrap_or_default();

    for item in data {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        if include_author {
            if let Some(author) = object
                .get("authorId")
                .and_then(Value::as_str)
                .and_then(|id| authors.get(id))
            {
                object.insert("author".to_owned(), author.clone());
            }
        }
        if include_stats {
            if let Some(latest_stats) = object
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| stats.get(id))
            {
                object.insert("latestStats".to_owned(), latest_stats.clone());
            }
        }
        if include_media {
            let media_items = object
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| media.get(id))
                .cloned()
                .unwrap_or_default();
            object.insert("media".to_owned(), Value::Array(media_items));
        }
    }

    Ok(())
}

async fn hydrate_browse_tweet_list(pool: &sqlx::PgPool, data: &mut [Value]) -> AppResult<()> {
    let includes = IncludeSet::from_values(&["author", "stats", "media-resources"]);
    hydrate_tweet_list(pool, data, &includes).await
}

async fn fetch_author_summary_map(
    pool: &sqlx::PgPool,
    author_ids: &[i64],
) -> AppResult<HashMap<String, Value>> {
    if author_ids.is_empty() {
        return Ok(HashMap::new());
    }

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
        WHERE u.id = ANY($1::BIGINT[])
        "#,
    )
    .bind(author_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let id = row.get::<i64, _>("id");
            (
                id.to_string(),
                json!({
                    "id": json_i64(id),
                    "registeredAt": row_time_opt(&row, "registered_at"),
                    "createdAt": row_time(&row, "created_at"),
                    "updatedAt": row_time(&row, "updated_at"),
                    "latestSnapshot": row.get::<Option<Value>, _>("latest_snapshot").unwrap_or(Value::Null),
                    "latestStats": row.get::<Option<Value>, _>("latest_stats").unwrap_or(Value::Null),
                }),
            )
        })
        .collect())
}

async fn fetch_author_summary(pool: &sqlx::PgPool, author_id: i64) -> AppResult<Option<Value>> {
    let mut authors = fetch_author_summary_map(pool, &[author_id]).await?;
    Ok(authors.remove(&author_id.to_string()))
}

async fn fetch_tweet_stats_map(
    pool: &sqlx::PgPool,
    tweet_ids: &[i64],
) -> AppResult<HashMap<String, Value>> {
    if tweet_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT tweet_id, to_jsonb(stats) AS data
        FROM tweet.v_latest_tweet_stats AS stats
        WHERE tweet_id = ANY($1::BIGINT[])
        "#,
    )
    .bind(tweet_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("tweet_id").to_string(),
                row.get::<Value, _>("data"),
            )
        })
        .collect())
}

async fn fetch_tweet_media_map(
    pool: &sqlx::PgPool,
    tweet_ids: &[i64],
    include_latest_resource: bool,
) -> AppResult<HashMap<String, Vec<Value>>> {
    if tweet_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            ref.tweet_id,
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
            to_jsonb(resource) AS latest_resource,
            warnings.sensitivity_warnings,
            transfer.status::text AS transfer_status,
            transfer.storage_object_id,
            transfer.object_key AS storage_object_key,
            media.created_at,
            media.updated_at
        FROM tweet.tweet_media_ref AS ref
        INNER JOIN tweet.media AS media
          ON media.id = ref.media_id
        LEFT JOIN tweet.v_latest_media_resource AS resource
          ON resource.media_id = media.id
        LEFT JOIN media.v_latest_transfer_overview AS transfer
          ON transfer.media_id = media.id
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(dict.value ORDER BY warning.ord) FILTER (WHERE dict.value IS NOT NULL) AS sensitivity_warnings
            FROM unnest(media.sensitivity_warning_ids) WITH ORDINALITY AS warning(id, ord)
            LEFT JOIN tweet.string_dict AS dict
              ON dict.id = warning.id
        ) AS warnings ON TRUE
        WHERE ref.tweet_id = ANY($1::BIGINT[])
        ORDER BY ref.tweet_id ASC, ref.display_order ASC, media.id ASC
        "#,
    )
    .bind(tweet_ids)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::<String, Vec<Value>>::new();
    for row in rows {
        let tweet_id = row.get::<i64, _>("tweet_id").to_string();
        let mut item = media_json_from_row(&row);
        if let Some(object) = item.as_object_mut() {
            object.insert(
                "displayOrder".to_owned(),
                json!(row.get::<i16, _>("display_order")),
            );
            object.insert(
                "sensitivityWarnings".to_owned(),
                row.get::<Option<Value>, _>("sensitivity_warnings")
                    .unwrap_or_else(|| json!([])),
            );
            insert_media_transfer_fields(object, &row);
            if include_latest_resource {
                object.insert(
                    "latestResource".to_owned(),
                    row.get::<Option<Value>, _>("latest_resource")
                        .unwrap_or(Value::Null),
                );
            }
        }
        map.entry(tweet_id).or_default().push(item);
    }

    Ok(map)
}

fn insert_media_transfer_fields(object: &mut Map<String, Value>, row: &sqlx::postgres::PgRow) {
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
    object.insert(
        "storageObjectKey".to_owned(),
        row.get::<Option<String>, _>("storage_object_key")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
}

fn string_i64_field(item: &Value, field: &str) -> Option<i64> {
    item.get(field)?.as_str()?.parse().ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tweet_list_sort() {
        assert_eq!(
            TweetListSort::parse(None).unwrap(),
            TweetListSort::PublishedAt
        );
        assert_eq!(
            TweetListSort::parse(Some("createdAt")).unwrap(),
            TweetListSort::CreatedAt
        );
        assert_eq!(
            TweetListSort::parse(Some("updatedAt")).unwrap(),
            TweetListSort::UpdatedAt
        );
        assert!(TweetListSort::parse(Some("time")).is_err());
    }

    #[test]
    fn tweet_cursor_accepts_legacy_published_at() {
        let cursor: TweetCursor = serde_json::from_value(json!({
            "v": CURSOR_VERSION,
            "q": null,
            "author_id": null,
            "relation": "all",
            "published_at": "2026-04-10T08:30:00Z",
            "id": 1
        }))
        .unwrap();

        assert_eq!(cursor.sort, TweetListSort::PublishedAt);
        assert!(cursor.sort_at.is_none());
        assert!(cursor.sort_time().is_ok());
    }

    #[test]
    fn tweet_cursor_serializes_sort_time() {
        let cursor = TweetCursor {
            v: CURSOR_VERSION,
            q: Some("12".to_owned()),
            author_id: Some(42),
            relation: "all".to_owned(),
            sort: TweetListSort::UpdatedAt,
            sort_at: Some(time::macros::datetime!(2026-04-10 08:30:00 UTC)),
            published_at: None,
            id: 1,
        };
        let value = serde_json::to_value(cursor).unwrap();

        assert_eq!(value["sort"], "updatedAt");
        assert_eq!(value["sort_at"], "2026-04-10T08:30:00Z");
        assert!(value.get("published_at").is_none());
    }

    #[test]
    fn tweet_search_cursor_serializes_sort_and_offset() {
        let cursor = TweetSearchCursor {
            v: CURSOR_VERSION,
            q: Some("rust".to_owned()),
            tweet_ids: vec![100],
            author_ids: vec![42],
            author_user_names: vec!["alice".to_owned()],
            relation: "all".to_owned(),
            sort: TweetSearchSort::UpdatedAt,
            offset: 30,
        };
        let value = serde_json::to_value(cursor).unwrap();

        assert_eq!(value["q"], "rust");
        assert_eq!(value["tweet_ids"][0], 100);
        assert_eq!(value["author_ids"][0], 42);
        assert_eq!(value["author_user_names"][0], "alice");
        assert_eq!(value["sort"], "updatedAt");
        assert_eq!(value["offset"], 30);
    }

    #[test]
    fn tweet_search_query_spec_parses_exact_filters() {
        let spec = tweet_query_spec_from_search_query(TweetSearchListQuery {
            list: ListQuery {
                q: Some("rust search".to_owned()),
                limit: Some(25),
                ..Default::default()
            },
            tweet_ids: Some("2,1,2".to_owned()),
            author_ids: Some("42".to_owned()),
            author_user_names: Some("@Alice,bob".to_owned()),
            author_id: Some("7".to_owned()),
            relation: Some("reply".to_owned()),
            sort: None,
        })
        .unwrap();

        assert_eq!(spec.q.as_deref(), Some("rust search"));
        assert_eq!(spec.tweet_ids, vec![1, 2]);
        assert_eq!(spec.author_ids, vec![7, 42]);
        assert_eq!(spec.author_user_names, vec!["alice", "bob"]);
        assert_eq!(spec.relation, "reply");
        assert_eq!(spec.sort, TweetSearchSort::Relevance);
        assert_eq!(spec.limit, 25);
    }

    #[test]
    fn tweet_search_query_spec_defaults_to_published_time_without_text() {
        let spec = tweet_query_spec_from_search_query(TweetSearchListQuery {
            list: ListQuery::default(),
            tweet_ids: None,
            author_ids: None,
            author_user_names: None,
            author_id: None,
            relation: None,
            sort: None,
        })
        .unwrap();

        assert!(spec.q.is_none());
        assert_eq!(spec.sort, TweetSearchSort::PublishedAt);
    }
}
