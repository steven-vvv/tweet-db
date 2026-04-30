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
            (SELECT COUNT(*) FROM media.storage_object) AS storage_objects,
            (SELECT COUNT(*) FROM search.index_queue) AS search_index_tasks
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let transfer_counts = sqlx::query(
        r#"
        SELECT
            COUNT(*) AS total,
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

    let search_counts = sqlx::query(
        r#"
        SELECT
            COUNT(*) AS total,
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (WHERE status = 'processing') AS processing,
            COUNT(*) FILTER (WHERE status = 'completed') AS completed,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed
        FROM search.index_queue
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(detail_response(
        json!({
            "identity": {
                "users": {
                    "total": account_counts.get::<i64, _>("total"),
                    "active": account_counts.get::<i64, _>("active"),
                    "disabled": account_counts.get::<i64, _>("disabled"),
                    "admins": account_counts.get::<i64, _>("admins"),
                },
            },
            "tweet": {
                "twitterUsers": domain_counts.get::<i64, _>("twitter_users"),
                "tweets": domain_counts.get::<i64, _>("tweets"),
                "media": domain_counts.get::<i64, _>("media"),
            },
            "storage": {
                "provider": state.settings.config.storage.provider,
                "bucket": state.settings.config.storage.bucket,
                "objects": domain_counts.get::<i64, _>("storage_objects"),
            },
            "transfer": {
                "enabled": state.settings.config.transfer.enabled,
                "workerCount": state.settings.config.transfer.worker_count,
                "tasks": {
                    "total": transfer_counts.get::<i64, _>("total"),
                    "pending": transfer_counts.get::<i64, _>("pending"),
                    "processing": transfer_counts.get::<i64, _>("processing"),
                    "completed": transfer_counts.get::<i64, _>("completed"),
                    "failed": transfer_counts.get::<i64, _>("failed"),
                    "canceled": transfer_counts.get::<i64, _>("canceled"),
                },
            },
            "search": {
                "enabled": state.settings.config.search.enabled,
                "indexTasks": {
                    "total": search_counts.get::<i64, _>("total"),
                    "pending": search_counts.get::<i64, _>("pending"),
                    "processing": search_counts.get::<i64, _>("processing"),
                    "completed": search_counts.get::<i64, _>("completed"),
                    "failed": search_counts.get::<i64, _>("failed"),
                },
            },
        }),
        Default::default(),
    )))
}
