use serde_json::{Value, json};
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
        "legacyText": row.get::<Value, _>("legacy_text"),
        "noteId": row.get::<Option<String>, _>("note_id"),
        "noteText": row.get::<Option<Value>, _>("note_text"),
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
