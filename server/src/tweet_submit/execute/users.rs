use super::*;

pub(super) async fn write_combined_user_batch(
    store: &TweetStore<'_>,
    users: &[Indexed<TwitterUser>],
    tweet_authors: &[Indexed<TwitterUser>],
    user_results: &mut [ObjectResultBuilder],
    tweet_results: &mut [ObjectResultBuilder],
) {
    if users.is_empty() && tweet_authors.is_empty() {
        return;
    }

    #[derive(Clone)]
    struct CombinedUserWrite {
        value: TwitterUser,
        user_indices: Vec<usize>,
        tweet_indices: Vec<usize>,
    }

    let mut combined = HashMap::<i64, CombinedUserWrite>::new();
    for item in users {
        combined
            .entry(item.value.id)
            .and_modify(|entry| {
                if entry.value.registered_at.is_none() && item.value.registered_at.is_some() {
                    entry.value.registered_at = item.value.registered_at;
                }
                entry.user_indices.push(item.index);
            })
            .or_insert_with(|| CombinedUserWrite {
                value: item.value.clone(),
                user_indices: vec![item.index],
                tweet_indices: Vec::new(),
            });
    }
    for item in tweet_authors {
        combined
            .entry(item.value.id)
            .and_modify(|entry| entry.tweet_indices.push(item.index))
            .or_insert_with(|| CombinedUserWrite {
                value: item.value.clone(),
                user_indices: Vec::new(),
                tweet_indices: vec![item.index],
            });
    }

    let combined = combined.into_values().collect::<Vec<_>>();
    let values = combined
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.insert_users_changed(&values).await {
        Ok(changed) => {
            for item in &combined {
                if changed.contains(&item.value.id) {
                    for index in &item.user_indices {
                        user_results[*index].accepted("twitter_user", "inserted_or_filled");
                    }
                    for index in &item.tweet_indices {
                        tweet_results[*index].accepted("tweet_author", "inserted_minimal");
                    }
                } else {
                    for index in &item.user_indices {
                        user_results[*index].skipped("twitter_user", "unchanged_or_existing");
                    }
                    for index in &item.tweet_indices {
                        tweet_results[*index].skipped("tweet_author", "existing");
                    }
                }
            }
        }
        Err(error) => {
            for item in &combined {
                for index in &item.user_indices {
                    user_results[*index].failed("twitter_user", error.to_string());
                }
                for index in &item.tweet_indices {
                    tweet_results[*index].failed("tweet_author", error.to_string());
                }
            }
        }
    }
}

pub(super) async fn write_user_snapshots_batch(
    state: &AppState,
    store: &TweetStore<'_>,
    items: &[Indexed<UserSnapshot>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.append_user_snapshots_if_changed_many(&values).await {
        Ok(statuses) => {
            let mut index_targets = Vec::new();
            for item in items {
                let key = (item.value.user_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "user_snapshot", write);
                        if write == ConditionalWrite::Inserted {
                            index_targets.push((
                                item.index,
                                crate::search::IndexTarget::user(item.value.user_id),
                            ));
                        }
                    }
                    None => results[item.index]
                        .failed("user_snapshot", "missing batch write status".to_owned()),
                }
            }
            enqueue_search_targets(state, &index_targets, results, "search_user_index").await;
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("user_snapshot", error.to_string());
            }
        }
    }
}

async fn enqueue_search_targets(
    state: &AppState,
    targets: &[(usize, crate::search::IndexTarget)],
    results: &mut [ObjectResultBuilder],
    operation_name: &'static str,
) {
    if state.search.is_none() || targets.is_empty() {
        return;
    }

    let values = targets
        .iter()
        .map(|(_, target)| *target)
        .collect::<Vec<_>>();
    match crate::search::enqueue_targets(&state.db, &values).await {
        Ok(_) => {
            for (index, _) in targets {
                results[*index].accepted(operation_name, "queued");
            }
        }
        Err(error) => {
            for (index, _) in targets {
                results[*index].failed(operation_name, error.to_string());
            }
        }
    }
}

pub(super) async fn write_user_stats_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<UserStats>],
    results: &mut [ObjectResultBuilder],
    stats_interval: i64,
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store
        .append_user_stats_if_changed_many(&values, stats_interval)
        .await
    {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.user_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "user_stats", write)
                    }
                    None => results[item.index].failed("user_stats", "missing batch write status"),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("user_stats", error.to_string());
            }
        }
    }
}
