use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    state::AppState,
    string_dict::{StringDictCache, StringDictValue, StringSemantic},
    tweet_model::{Hashtag, Symbol, TweetMediaRef, UserCategory},
    tweet_store::TweetStore,
};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryTweetRequest {
    #[serde(default)]
    pub users: Vec<QueryIdSelector>,
    #[serde(default)]
    pub tweets: Vec<QueryIdSelector>,
    #[serde(default)]
    pub media: Vec<QueryIdSelector>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryIdSelector {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryTweetResponse {
    pub summary: QuerySummary,
    pub users: Vec<QueryObjectResult>,
    pub tweets: Vec<QueryObjectResult>,
    pub media: Vec<QueryObjectResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuerySummary {
    pub total: usize,
    pub found: usize,
    pub missing: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryObjectResult {
    pub id: Option<String>,
    pub status: QueryObjectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryObjectStatus {
    Found,
    Missing,
    Failed,
}

pub async fn query_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<QueryTweetRequest>,
) -> AppResult<Json<QueryTweetResponse>> {
    let _session = auth::require_registered_session(session)?;
    let total = payload.users.len() + payload.tweets.len() + payload.media.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "query item count {total} exceeds ingest.max_items_per_batch"
        )));
    }

    let store = TweetStore::new(&state.db, &state.string_dict);
    let (users, tweets, media) = tokio::try_join!(
        query_users(&store, &state.string_dict, &payload.users),
        query_tweet_objects(&store, &state.string_dict, &payload.tweets),
        query_media_objects(&store, &state.string_dict, &payload.media),
    )?;

    let mut response = QueryTweetResponse {
        users,
        tweets,
        media,
        ..Default::default()
    };
    response.summary = summarize_results(&response);
    Ok(Json(response))
}

mod build;
mod db_types;
mod decode;
mod fetch;

use self::{build::*, db_types::*, decode::*, fetch::*};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_request_accepts_three_root_lists() {
        let request: QueryTweetRequest = serde_json::from_value(json!({
            "users": [],
            "tweets": [],
            "media": []
        }))
        .unwrap();

        assert_eq!(request.users.len(), 0);
        assert_eq!(request.tweets.len(), 0);
        assert_eq!(request.media.len(), 0);
    }

    #[test]
    fn summary_counts_found_missing_and_failed_results() {
        let response = QueryTweetResponse {
            users: vec![found_result(Some("1".to_owned()), json!({"id": "1"}))],
            tweets: vec![missing_result(Some("2".to_owned()))],
            media: vec![failed_result(Some("3".to_owned()), "broken")],
            ..Default::default()
        };

        let summary = summarize_results(&response);

        assert_eq!(summary.total, 3);
        assert_eq!(summary.found, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn x_monkey_remote_db_query_fixture_matches_response_contract() {
        let response: QueryTweetResponse = serde_json::from_str(include_str!(
            "../../test-fixtures/x_monkey_remote_db_query_response.json"
        ))
        .unwrap();

        assert_eq!(response.summary.total, 3);
        assert_eq!(response.summary.found, 3);
        assert_eq!(response.tweets[0].status, QueryObjectStatus::Found);
        assert_eq!(
            response.tweets[0].data.as_ref().unwrap()["edit"]["versionIds"][0],
            "1912345678901234500"
        );
        assert_eq!(
            response.media[0].data.as_ref().unwrap()["resource"]["availability"],
            "Available"
        );
    }
}
