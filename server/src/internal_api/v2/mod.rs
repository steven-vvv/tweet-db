use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

mod audit;
mod common;
mod identity;
mod media;
mod rows;
mod search;
mod storage;
mod system;
mod transfer;
mod tweets;
mod twitter_users;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/internal/v2/me", get(common::me))
        .route("/internal/v2/identity/users", get(identity::list_users))
        .route(
            "/internal/v2/identity/users/{user_id}",
            get(identity::get_user).patch(identity::patch_user),
        )
        .route(
            "/internal/v2/identity/users/{user_id}/sessions",
            get(identity::list_user_sessions),
        )
        .route(
            "/internal/v2/identity/users/{user_id}/sso-authorizations",
            get(identity::list_user_sso_authorizations),
        )
        .route("/internal/v2/audit/events", get(audit::list_events))
        .route(
            "/internal/v2/audit/events/{event_id}",
            get(audit::get_event),
        )
        .route(
            "/internal/v2/twitter-users",
            get(twitter_users::list_twitter_users),
        )
        .route(
            "/internal/v2/twitter-users/{user_id}",
            get(twitter_users::get_twitter_user),
        )
        .route(
            "/internal/v2/twitter-users/{user_id}/snapshots",
            get(twitter_users::list_twitter_user_snapshots),
        )
        .route(
            "/internal/v2/twitter-users/{user_id}/stats",
            get(twitter_users::list_twitter_user_stats),
        )
        .route("/internal/v2/tweets", get(tweets::list_tweets))
        .route("/internal/v2/tweets/{tweet_id}", get(tweets::get_tweet))
        .route(
            "/internal/v2/tweets/{tweet_id}/media",
            get(tweets::list_tweet_media),
        )
        .route("/internal/v2/media", get(media::list_media))
        .route("/internal/v2/media/{media_id}", get(media::get_media))
        .route(
            "/internal/v2/media/{media_id}/open",
            get(media::open_media_storage_object),
        )
        .route(
            "/internal/v2/media/{media_id}/resources",
            get(media::list_media_resources),
        )
        .route(
            "/internal/v2/media/{media_id}/tweets",
            get(media::list_media_tweets),
        )
        .route(
            "/internal/v2/media/{media_id}/transfer-tasks",
            get(media::list_media_transfer_tasks).post(media::create_media_transfer_task),
        )
        .route(
            "/internal/v2/storage/objects",
            get(storage::list_storage_objects),
        )
        .route(
            "/internal/v2/storage/objects/{object_id}",
            get(storage::get_storage_object),
        )
        .route(
            "/internal/v2/storage/objects/{object_id}/transfer-tasks",
            get(storage::list_storage_object_transfer_tasks),
        )
        .route(
            "/internal/v2/storage/objects/{object_id}/presigned-url",
            post(storage::create_storage_object_presigned_url),
        )
        .route("/internal/v2/transfer/tasks", get(transfer::list_tasks))
        .route(
            "/internal/v2/transfer/tasks/{task_id}",
            get(transfer::get_task),
        )
        .route(
            "/internal/v2/transfer/tasks/{task_id}/transitions",
            post(transfer::transition_task),
        )
        .route(
            "/internal/v2/search/index-tasks",
            get(search::list_index_tasks).post(search::enqueue_index_tasks),
        )
        .route(
            "/internal/v2/search/index-tasks/{task_id}",
            get(search::get_index_task),
        )
        .route("/internal/v2/search/tweets", get(tweets::search_tweets))
        .route("/internal/v2/system/summary", get(system::summary))
}
