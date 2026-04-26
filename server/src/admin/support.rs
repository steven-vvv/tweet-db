use super::*;

pub(super) fn list_response(items: Vec<Value>, next_cursor: Option<String>) -> Value {
    json!({
        "items": items,
        "nextCursor": next_cursor,
    })
}

pub(super) fn detail_response(summary: Value, record: Value, related: Value) -> Value {
    json!({
        "summary": summary,
        "record": record,
        "related": related,
    })
}

pub(super) fn resolve_limit(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
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

pub(super) fn paginate_rows<T, C, F>(
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
