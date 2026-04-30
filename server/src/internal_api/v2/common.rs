use axum::{
    Json,
    extract::{Extension, Query},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{DeserializeOwned, Error as DeError},
};
use serde_json::{Map, Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
};

pub(super) const DEFAULT_LIMIT: usize = 50;
pub(super) const MAX_LIMIT: usize = 100;
pub(super) const CURSOR_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Capability {
    IdentityRead,
    IdentityWrite,
    TweetRead,
    MediaRead,
    MediaTransferWrite,
    StorageRead,
    TransferWrite,
    SearchRead,
    SearchWrite,
    AuditRead,
    SystemRead,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListQuery {
    pub cursor: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_usize")]
    pub limit: Option<usize>,
    pub q: Option<String>,
    pub include: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ListResponse {
    pub data: Vec<Value>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize)]
pub(super) struct DetailResponse {
    pub data: Value,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub included: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct ActionResponse {
    pub data: Value,
    pub result: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Pagination {
    pub limit: usize,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct IncludeSet {
    values: Vec<String>,
}

pub async fn me(
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<MeQuery>,
) -> AppResult<Json<DetailResponse>> {
    let session = require_capability(session, Capability::SystemRead)?;
    let includes = IncludeSet::parse(query.include.as_deref())?;
    let mut included = Map::new();
    if includes.contains("capabilities") {
        included.insert("capabilities".to_owned(), json!(all_capability_names()));
    }

    Ok(Json(detail_response(
        json!({
            "authenticated": true,
            "registered": true,
            "userId": session.record.user_id,
            "username": session.record.username,
            "isAdmin": session.record.user_is_admin,
            "disabled": session.record.user_disabled_at.is_some(),
            "subjectId": session.record.sso_subject_id,
            "authorizationId": session.record.authorization_id,
            "sessionId": session.record.selector,
            "expiresAt": format_time(session.record.expires_at),
        }),
        included,
    )))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct MeQuery {
    include: Option<String>,
}

pub(super) fn require_capability(
    session: Option<Extension<ActiveSession>>,
    _capability: Capability,
) -> AppResult<ActiveSession> {
    auth::require_admin_session(session)
}

pub(super) fn list_response(
    data: Vec<Value>,
    limit: usize,
    next_cursor: Option<String>,
) -> ListResponse {
    ListResponse {
        data,
        pagination: Pagination { limit, next_cursor },
    }
}

pub(super) fn detail_response(data: Value, included: Map<String, Value>) -> DetailResponse {
    DetailResponse { data, included }
}

pub(super) fn action_response(data: Value, result: Value) -> ActionResponse {
    ActionResponse { data, result }
}

pub(super) fn resolve_limit(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn deserialize_optional_usize<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum LimitValue {
        Number(usize),
        Text(String),
    }

    let value = Option::<LimitValue>::deserialize(deserializer)?;
    match value {
        Some(LimitValue::Number(value)) => Ok(Some(value)),
        Some(LimitValue::Text(value)) => {
            let value = value.trim();
            if value.is_empty() {
                Ok(None)
            } else {
                value
                    .parse::<usize>()
                    .map(Some)
                    .map_err(|_| D::Error::custom("limit must be a positive integer"))
            }
        }
        None => Ok(None),
    }
}

pub(super) fn limit_plus_one(limit: usize) -> i64 {
    (limit.saturating_add(1)) as i64
}

pub(super) fn normalize_query(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn like_pattern(value: Option<&str>) -> Option<String> {
    value.map(|value| format!("%{value}%"))
}

pub(super) fn prefix_pattern(value: Option<&str>) -> Option<String> {
    value.map(|value| format!("{value}%"))
}

pub(super) fn lowercase_prefix_pattern(value: Option<&str>) -> Option<String> {
    value.map(|value| format!("{}%", value.to_ascii_lowercase()))
}

pub(super) fn parse_optional_i64(value: Option<&str>, field: &str) -> AppResult<Option<i64>> {
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

pub(super) fn normalize_user_status(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "active" | "disabled" => Ok(value),
        _ => Err(AppError::bad_request(
            "status must be one of all, active, disabled",
        )),
    }
}

pub(super) fn normalize_transfer_status(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "pending" | "processing" | "completed" | "failed" | "canceled" => Ok(value),
        _ => Err(AppError::bad_request(
            "status must be one of all, pending, processing, completed, failed, canceled",
        )),
    }
}

pub(super) fn normalize_media_type(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "photo" | "video" | "animated_gif" => Ok(value),
        _ => Err(AppError::bad_request(
            "mediaType must be one of all, photo, video, animated_gif",
        )),
    }
}

pub(super) fn normalize_tweet_relation(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "original" | "reply" | "quote" | "repost" => Ok(value),
        _ => Err(AppError::bad_request(
            "relation must be one of all, original, reply, quote, repost",
        )),
    }
}

pub(super) fn normalize_index_target_kind(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "user" | "tweet" => Ok(value),
        _ => Err(AppError::bad_request(
            "targetKind must be one of all, user, tweet",
        )),
    }
}

pub(super) fn ensure_cursor_version(version: u8) -> AppResult<()> {
    if version == CURSOR_VERSION {
        Ok(())
    } else {
        Err(AppError::bad_request("unsupported cursor version"))
    }
}

pub(super) fn ensure_filter_match(
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

pub(super) fn paginate_rows<C, F>(
    mut rows: Vec<sqlx::postgres::PgRow>,
    limit: usize,
    map: F,
) -> AppResult<(Vec<Value>, Option<String>)>
where
    C: Serialize,
    F: Fn(sqlx::postgres::PgRow) -> (Value, C),
{
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    let mut data = Vec::with_capacity(rows.len());
    let mut last_cursor = None;

    for row in rows {
        let (item, cursor) = map(row);
        data.push(item);
        last_cursor = Some(encode_cursor(&cursor)?);
    }

    Ok((data, has_more.then_some(last_cursor).flatten()))
}

pub(super) fn encode_cursor<T: Serialize>(value: &T) -> AppResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(super) fn decode_cursor<T: DeserializeOwned>(raw: Option<&str>) -> AppResult<Option<T>> {
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

pub(super) fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

pub(super) fn format_time_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_time)
}

pub(super) fn row_time(row: &sqlx::postgres::PgRow, column: &str) -> String {
    use sqlx::Row;
    format_time(row.get(column))
}

pub(super) fn row_time_opt(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    use sqlx::Row;
    format_time_opt(row.get(column))
}

impl IncludeSet {
    pub(super) fn parse(raw: Option<&str>) -> AppResult<Self> {
        let values = raw
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();

        Ok(Self { values })
    }

    pub(super) fn contains(&self, value: &str) -> bool {
        self.values.iter().any(|item| item == value)
    }
}

pub(super) fn json_i64(value: i64) -> String {
    value.to_string()
}

pub(super) fn json_i64_opt(value: Option<i64>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn all_capability_names() -> Vec<&'static str> {
    vec![
        "identity.read",
        "identity.write",
        "tweet.read",
        "media.read",
        "media.transfer.write",
        "storage.read",
        "transfer.write",
        "search.read",
        "search.write",
        "audit.read",
        "system.read",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_limits() {
        assert_eq!(resolve_limit(None), DEFAULT_LIMIT);
        assert_eq!(resolve_limit(Some(0)), 1);
        assert_eq!(resolve_limit(Some(500)), MAX_LIMIT);
    }

    #[test]
    fn parses_include_set() {
        let includes = IncludeSet::parse(Some("stats, media ,")).unwrap();
        assert!(includes.contains("stats"));
        assert!(includes.contains("media"));
        assert!(!includes.contains("audit"));
    }
}
