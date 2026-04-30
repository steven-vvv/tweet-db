use serde_json::{Map, Value, json};
use sqlx::Row;
use uuid::Uuid;

use super::common::*;

pub(super) fn transfer_task_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "mediaId": json_i64(row.get::<i64, _>("media_id")),
        "sourceRecordedAt": row_time(row, "source_recorded_at"),
        "sourceUrl": row.get::<String, _>("source_url"),
        "sourceKind": row.get::<String, _>("source_kind"),
        "sourceContentType": row.get::<Option<String>, _>("source_content_type"),
        "status": row.get::<String, _>("status"),
        "attemptCount": row.get::<i32, _>("attempt_count"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "claimedBy": row.get::<Option<String>, _>("claimed_by"),
        "claimedAt": row_time_opt(row, "claimed_at"),
        "completedAt": row_time_opt(row, "completed_at"),
        "storageObjectId": row.get::<Option<Uuid>, _>("storage_object_id"),
        "storageObjectKey": row.get::<Option<String>, _>("storage_object_key"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}

pub(super) fn storage_object_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "provider": row.get::<String, _>("provider"),
        "bucket": row.get::<String, _>("bucket"),
        "objectKey": row.get::<String, _>("object_key"),
        "contentType": row.get::<String, _>("content_type"),
        "contentLength": row.get::<i64, _>("content_length"),
        "etag": row.get::<Option<String>, _>("etag"),
        "sha256Hex": row.get::<String, _>("sha256_hex"),
        "createdAt": row_time(row, "created_at"),
        "taskCount": row.try_get::<i64, _>("task_count").unwrap_or_default(),
    })
}

pub(super) fn media_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": json_i64(row.get::<i64, _>("id")),
        "type": row.get::<String, _>("media_type"),
        "altText": row.get::<Option<String>, _>("alt_text"),
        "grokPostId": row.get::<Option<Uuid>, _>("grok_post_id"),
        "geometry": row.get::<Option<Value>, _>("geometry"),
        "sizeVariants": row.get::<Option<Value>, _>("size_variants"),
        "taggedUsers": row.get::<Value, _>("tagged_users"),
        "sensitivityWarningIds": row.get::<Value, _>("sensitivity_warning_ids"),
        "originTweetId": json_i64_opt(row.get::<Option<i64>, _>("origin_tweet_id")),
        "originUserId": json_i64_opt(row.get::<Option<i64>, _>("origin_user_id")),
        "details": row.get::<Option<Value>, _>("details"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}

pub(super) fn tweet_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": json_i64(row.get::<i64, _>("id")),
        "publishedAt": row_time(row, "published_at"),
        "sourceId": row.get::<Option<i16>, _>("source_id"),
        "authorId": json_i64(row.get::<i64, _>("author_id")),
        "placeId": row.get::<Option<String>, _>("place_id"),
        "legacyText": annotated_text_json(row.get::<Value, _>("legacy_text")),
        "noteId": row.get::<Option<String>, _>("note_id"),
        "noteText": row.get::<Option<Value>, _>("note_text").map(annotated_text_json),
        "languageId": row.get::<Option<i16>, _>("language_id"),
        "conversationId": json_i64(row.get::<i64, _>("conversation_id")),
        "replyToTweetId": json_i64_opt(row.get::<Option<i64>, _>("reply_to_tweet_id")),
        "replyToUserId": json_i64_opt(row.get::<Option<i64>, _>("reply_to_user_id")),
        "quoteTweetId": json_i64_opt(row.get::<Option<i64>, _>("quote_tweet_id")),
        "quotePermalink": row.get::<Option<Value>, _>("quote_permalink"),
        "repostId": json_i64_opt(row.get::<Option<i64>, _>("repost_id")),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}

pub(super) fn twitter_user_json_from_row(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": json_i64(row.get::<i64, _>("id")),
        "registeredAt": row_time_opt(row, "registered_at"),
        "createdAt": row_time(row, "created_at"),
        "updatedAt": row_time(row, "updated_at"),
    })
}

fn annotated_text_json(raw: Value) -> Value {
    let Some(object) = raw.as_object() else {
        return raw;
    };

    json!({
        "text": object.get("body").cloned().unwrap_or(Value::Null),
        "displayRange": range_from_object(object, "display_range_start", "display_range_end"),
        "entities": {
            "hashtags": map_entity_array(object.get("hashtags"), hashtag_entity_json),
            "symbols": map_entity_array(object.get("symbols"), symbol_entity_json),
            "urls": map_entity_array(object.get("urls"), url_entity_json),
            "mentions": map_entity_array(object.get("mentions"), mention_entity_json),
            "media": map_entity_array(object.get("media_refs"), media_entity_json),
        },
        "styles": map_entity_array(object.get("styles"), style_range_json),
    })
}

fn map_entity_array(value: Option<&Value>, map: fn(&Map<String, Value>) -> Value) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(map)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn hashtag_entity_json(object: &Map<String, Value>) -> Value {
    json!({
        "hashtagId": string_from_value(object.get("hashtag_id")),
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn symbol_entity_json(object: &Map<String, Value>) -> Value {
    json!({
        "symbolId": string_from_value(object.get("symbol_id")),
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn url_entity_json(object: &Map<String, Value>) -> Value {
    json!({
        "url": object.get("url").cloned().unwrap_or(Value::Null),
        "expandedUrl": object.get("expanded_url").cloned().unwrap_or(Value::Null),
        "displayText": object.get("display_text").cloned().unwrap_or(Value::Null),
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn mention_entity_json(object: &Map<String, Value>) -> Value {
    json!({
        "userId": string_from_value(object.get("user_id")),
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn media_entity_json(object: &Map<String, Value>) -> Value {
    json!({
        "mediaId": string_from_value(object.get("media_id")),
        "displayText": object.get("display_text").cloned().unwrap_or(Value::Null),
        "expandedUrl": object.get("expanded_url").cloned().unwrap_or(Value::Null),
        "url": object.get("url").cloned().unwrap_or(Value::Null),
        "origin": {
            "tweetId": string_from_value(object.get("origin_tweet_id")),
            "userId": string_from_value(object.get("origin_user_id")),
        },
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn style_range_json(object: &Map<String, Value>) -> Value {
    json!({
        "styleIds": object.get("style_ids").cloned().unwrap_or_else(|| json!([])),
        "range": range_from_object(object, "range_start", "range_end"),
    })
}

fn range_from_object(object: &Map<String, Value>, start_key: &str, end_key: &str) -> Value {
    match (
        object.get(start_key).and_then(Value::as_i64),
        object.get(end_key).and_then(Value::as_i64),
    ) {
        (Some(start), Some(end)) => json!({ "start": start, "end": end }),
        _ => Value::Null,
    }
}

fn string_from_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Number(number)) => Value::String(number.to_string()),
        Some(Value::String(value)) => Value::String(value.clone()),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_text_json_maps_composite_fields() {
        let value = annotated_text_json(json!({
            "body": "hello https://t.co/a",
            "display_range_start": 0,
            "display_range_end": 20,
            "hashtags": [{"hashtag_id": 1, "range_start": 0, "range_end": 5}],
            "symbols": [],
            "urls": [{
                "url": "https://t.co/a",
                "expanded_url": "https://example.com/a",
                "display_text": "example.com/a",
                "range_start": 6,
                "range_end": 20
            }],
            "mentions": [{"user_id": 42, "range_start": 0, "range_end": 5}],
            "media_refs": [{"media_id": 7, "range_start": 6, "range_end": 20}],
            "styles": [{"range_start": 0, "range_end": 5, "style_ids": [3]}]
        }));

        assert_eq!(value["text"], "hello https://t.co/a");
        assert_eq!(value["displayRange"]["start"], 0);
        assert_eq!(value["entities"]["urls"][0]["displayText"], "example.com/a");
        assert_eq!(value["entities"]["mentions"][0]["userId"], "42");
        assert_eq!(value["entities"]["media"][0]["mediaId"], "7");
        assert_eq!(value["styles"][0]["styleIds"][0], 3);
    }
}
