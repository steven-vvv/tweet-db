use super::*;

pub async fn submit_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<SubmitTweetRequest>,
) -> AppResult<Json<SubmitTweetResponse>> {
    let _session = auth::require_admin_session(session)?;
    let total = payload.users.len() + payload.tweets.len() + payload.media.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "submit item count {total} exceeds ingest.max_items_per_batch"
        )));
    }

    let store = TweetStore::new(&state.db, &state.string_dict);
    let stats_interval = state.settings.config.ingest.stats_sample_interval_seconds;
    let mut prepared = prepare_submit_batch(&store, payload).await;
    execute_prepared_submit(&state, &store, &mut prepared, stats_interval).await;

    let mut response = SubmitTweetResponse {
        users: prepared
            .user_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        tweets: prepared
            .tweet_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        media: prepared
            .media_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        ..Default::default()
    };
    response.summary = summarize_results(&response);
    Ok(Json(response))
}
