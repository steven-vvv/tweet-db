use axum::{
    Json,
    extract::{Extension, State},
};
use serde_json::json;
use sqlx::Row;

use crate::{auth::ActiveSession, error::AppResult, state::AppState};

use super::common::*;

pub async fn summary(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<Json<DetailResponse>> {
    let _session = require_capability(session, Capability::SystemRead)?;
    let counts = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM iam.users) AS account_total,
            (SELECT COUNT(*) FILTER (WHERE disabled_at IS NULL) FROM iam.users) AS account_active,
            (SELECT COUNT(*) FILTER (WHERE disabled_at IS NOT NULL) FROM iam.users) AS account_disabled,
            (SELECT COUNT(*) FILTER (WHERE is_admin) FROM iam.users) AS account_admins,
            (SELECT COUNT(*) FROM tweet.twitter_user) AS twitter_users,
            (SELECT COUNT(*) FROM tweet.tweet) AS tweets,
            (SELECT COUNT(*) FROM tweet.media) AS media,
            (SELECT COUNT(*) FROM media.storage_object) AS storage_objects,
            (SELECT COUNT(*) FROM search.index_queue) AS search_index_tasks,
            (SELECT COUNT(*) FROM media.transfer_task) AS transfer_total,
            (SELECT COUNT(*) FILTER (WHERE status = 'pending') FROM media.transfer_task) AS transfer_pending,
            (SELECT COUNT(*) FILTER (WHERE status = 'processing') FROM media.transfer_task) AS transfer_processing,
            (SELECT COUNT(*) FILTER (WHERE status = 'completed') FROM media.transfer_task) AS transfer_completed,
            (SELECT COUNT(*) FILTER (WHERE status = 'failed') FROM media.transfer_task) AS transfer_failed,
            (SELECT COUNT(*) FILTER (WHERE status = 'canceled') FROM media.transfer_task) AS transfer_canceled,
            (SELECT COUNT(*) FROM search.index_queue) AS search_total,
            (SELECT COUNT(*) FILTER (WHERE status = 'pending') FROM search.index_queue) AS search_pending,
            (SELECT COUNT(*) FILTER (WHERE status = 'processing') FROM search.index_queue) AS search_processing,
            (SELECT COUNT(*) FILTER (WHERE status = 'completed') FROM search.index_queue) AS search_completed,
            (SELECT COUNT(*) FILTER (WHERE status = 'failed') FROM search.index_queue) AS search_failed
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(detail_response(
        json!({
            "identity": {
                "users": {
                    "total": counts.get::<i64, _>("account_total"),
                    "active": counts.get::<i64, _>("account_active"),
                    "disabled": counts.get::<i64, _>("account_disabled"),
                    "admins": counts.get::<i64, _>("account_admins"),
                },
            },
            "tweet": {
                "twitterUsers": counts.get::<i64, _>("twitter_users"),
                "tweets": counts.get::<i64, _>("tweets"),
                "media": counts.get::<i64, _>("media"),
            },
            "storage": {
                "provider": state.settings.config.storage.provider,
                "bucket": state.settings.config.storage.bucket,
                "objects": counts.get::<i64, _>("storage_objects"),
            },
            "transfer": {
                "enabled": state.settings.config.transfer.enabled,
                "workerCount": state.settings.config.transfer.worker_count,
                "tasks": {
                    "total": counts.get::<i64, _>("transfer_total"),
                    "pending": counts.get::<i64, _>("transfer_pending"),
                    "processing": counts.get::<i64, _>("transfer_processing"),
                    "completed": counts.get::<i64, _>("transfer_completed"),
                    "failed": counts.get::<i64, _>("transfer_failed"),
                    "canceled": counts.get::<i64, _>("transfer_canceled"),
                },
            },
            "search": {
                "enabled": state.settings.config.search.enabled,
                "indexTasks": {
                    "total": counts.get::<i64, _>("search_total"),
                    "pending": counts.get::<i64, _>("search_pending"),
                    "processing": counts.get::<i64, _>("search_processing"),
                    "completed": counts.get::<i64, _>("search_completed"),
                    "failed": counts.get::<i64, _>("search_failed"),
                },
            },
        }),
        Default::default(),
    )))
}
