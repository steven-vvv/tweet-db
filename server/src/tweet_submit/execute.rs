use super::*;

pub(super) async fn execute_prepared_submit(
    store: &TweetStore<'_>,
    prepared: &mut PreparedSubmitBatch,
    stats_interval: i64,
) {
    let snapshots = prepared
        .user_snapshots
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let places = prepared
        .tweet_places
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let tweets = prepared
        .tweets
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let policies = prepared
        .tweet_policies
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let notes = prepared
        .tweet_community_notes
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let media = prepared
        .media
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let resources = prepared
        .media_resources
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    if let Err(error) = store
        .preload_submit_batch_dicts(
            &snapshots, &places, &tweets, &policies, &notes, &media, &resources,
        )
        .await
    {
        let mut user_indices = HashSet::new();
        user_indices.extend(prepared.users.iter().map(|item| item.index));
        user_indices.extend(prepared.user_snapshots.iter().map(|item| item.index));
        user_indices.extend(prepared.user_stats.iter().map(|item| item.index));
        for index in user_indices {
            prepared.user_results[index].failed("dict_preload", error.to_string());
        }

        let mut tweet_indices = HashSet::new();
        tweet_indices.extend(prepared.tweet_authors.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_places.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweets.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_edits.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_policies.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_community_notes.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_stats.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_relations.iter().map(|item| item.index));
        for index in tweet_indices {
            prepared.tweet_results[index].failed("dict_preload", error.to_string());
        }

        let mut media_indices = HashSet::new();
        media_indices.extend(prepared.media.iter().map(|item| item.index));
        media_indices.extend(prepared.media_resources.iter().map(|item| item.index));
        for index in media_indices {
            prepared.media_results[index].failed("dict_preload", error.to_string());
        }
        return;
    }

    write_combined_user_batch(
        store,
        &prepared.users,
        &prepared.tweet_authors,
        &mut prepared.user_results,
        &mut prepared.tweet_results,
    )
    .await;
    write_user_snapshots_batch(store, &prepared.user_snapshots, &mut prepared.user_results).await;
    write_user_stats_batch(
        store,
        &prepared.user_stats,
        &mut prepared.user_results,
        stats_interval,
    )
    .await;
    write_media_batch(store, &prepared.media, &mut prepared.media_results).await;
    write_media_resources_batch(
        store,
        &prepared.media_resources,
        &mut prepared.media_results,
    )
    .await;
    write_tweet_places_batch(store, &prepared.tweet_places, &mut prepared.tweet_results).await;
    write_tweets_batch(store, &prepared.tweets, &mut prepared.tweet_results).await;
    write_tweet_edits_batch(store, &prepared.tweet_edits, &mut prepared.tweet_results).await;
    write_tweet_policies_batch(store, &prepared.tweet_policies, &mut prepared.tweet_results).await;
    write_tweet_community_notes_batch(
        store,
        &prepared.tweet_community_notes,
        &mut prepared.tweet_results,
    )
    .await;
    write_tweet_stats_batch(
        store,
        &prepared.tweet_stats,
        &mut prepared.tweet_results,
        stats_interval,
    )
    .await;
    replace_prepared_tweet_relations(store, prepared).await;
}

async fn write_combined_user_batch(
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

async fn write_user_snapshots_batch(
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
            for item in items {
                let key = (item.value.user_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "user_snapshot", write)
                    }
                    None => results[item.index]
                        .failed("user_snapshot", "missing batch write status".to_owned()),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("user_snapshot", error.to_string());
            }
        }
    }
}

async fn write_user_stats_batch(
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

async fn write_media_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<Media>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_media_changed(&values).await {
        Ok(changed) => {
            for item in items {
                if changed.contains(&item.value.id) {
                    results[item.index].accepted("media", "inserted_or_filled");
                } else {
                    results[item.index].skipped("media", "unchanged_or_existing");
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media", error.to_string());
            }
        }
    }
}

async fn write_media_resources_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<MediaResource>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.append_media_resources_if_changed_many(&values).await {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.media_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "media_resource", write)
                    }
                    None => {
                        results[item.index].failed("media_resource", "missing batch write status")
                    }
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media_resource", error.to_string());
            }
        }
    }
}

async fn write_tweet_places_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetPlace>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let mut deduped = HashMap::<String, (TweetPlace, Vec<usize>)>::new();
    for item in items {
        deduped
            .entry(item.value.id.clone())
            .and_modify(|entry| entry.1.push(item.index))
            .or_insert_with(|| (item.value.clone(), vec![item.index]));
    }
    let values = deduped
        .values()
        .map(|(value, _)| value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_places_changed(&values).await {
        Ok(changed) => {
            for (place_id, (_, indices)) in deduped {
                if changed.contains(&place_id) {
                    for index in indices {
                        results[index].accepted("tweet_place", "inserted_or_filled");
                    }
                } else {
                    for index in indices {
                        results[index].skipped("tweet_place", "unchanged_or_existing");
                    }
                }
            }
        }
        Err(error) => {
            for (_, (_, indices)) in deduped {
                for index in indices {
                    results[index].failed("tweet_place", error.to_string());
                }
            }
        }
    }
}

async fn write_tweets_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<Tweet>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.insert_tweets_changed(&values).await {
        Ok(changed) => {
            for item in items {
                if changed.contains(&item.value.id) {
                    results[item.index].accepted("tweet", "inserted_or_filled");
                } else {
                    results[item.index].skipped("tweet", "unchanged_or_existing");
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet", error.to_string());
            }
        }
    }
}

async fn write_tweet_edits_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetEdit>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_edits_write_statuses(&values).await {
        Ok(statuses) => {
            record_tweet_write_statuses(items, results, &statuses, "tweet_edit", |value| {
                value.tweet_id
            })
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_edit", error.to_string());
            }
        }
    }
}

async fn write_tweet_policies_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetPolicy>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_policies_write_statuses(&values).await {
        Ok(statuses) => {
            record_tweet_write_statuses(items, results, &statuses, "tweet_policy", |value| {
                value.tweet_id
            })
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_policy", error.to_string());
            }
        }
    }
}

async fn write_tweet_community_notes_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetCommunityNote>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store
        .upsert_tweet_community_notes_write_statuses(&values)
        .await
    {
        Ok(statuses) => record_tweet_write_statuses(
            items,
            results,
            &statuses,
            "tweet_community_note",
            |value| value.tweet_id,
        ),
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_community_note", error.to_string());
            }
        }
    }
}

async fn write_tweet_stats_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetStats>],
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
        .append_tweet_stats_if_changed_many(&values, stats_interval)
        .await
    {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.tweet_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "tweet_stats", write)
                    }
                    None => results[item.index].failed("tweet_stats", "missing batch write status"),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_stats", error.to_string());
            }
        }
    }
}

fn record_tweet_write_statuses<T>(
    items: &[Indexed<T>],
    results: &mut [ObjectResultBuilder],
    statuses: &HashMap<i64, ConditionalWrite>,
    operation: &'static str,
    key: impl Fn(&T) -> i64,
) {
    for item in items {
        match statuses.get(&key(&item.value)).copied() {
            Some(write) => record_conditional_write(&mut results[item.index], operation, write),
            None => results[item.index].failed(operation, "missing batch write status"),
        }
    }
}

fn record_relation_sync(
    result: &mut ObjectResultBuilder,
    operation: &'static str,
    status: Option<crate::tweet_store::RelationSyncStatus>,
) {
    use crate::tweet_store::RelationSyncStatus;

    match status {
        Some(RelationSyncStatus::Replaced) => result.accepted(operation, "replaced"),
        Some(RelationSyncStatus::ReplacedFiltered) => {
            result.accepted(operation, "replaced_with_missing_media_skipped")
        }
        Some(RelationSyncStatus::SkippedUnchanged) => result.skipped(operation, "unchanged"),
        Some(RelationSyncStatus::SkippedUnchangedFiltered) => {
            result.skipped(operation, "unchanged_with_missing_media_skipped")
        }
        Some(RelationSyncStatus::SkippedMissingTweet) => result.skipped(operation, "missing_tweet"),
        None => result.failed(operation, "missing relation sync status"),
    }
}

async fn replace_prepared_tweet_relations(
    store: &TweetStore<'_>,
    prepared: &mut PreparedSubmitBatch,
) {
    if prepared.tweet_relations.is_empty() {
        return;
    }

    let tweet_ids = prepared
        .tweet_relations
        .iter()
        .map(|item| item.tweet_id)
        .collect::<Vec<_>>();
    let media_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.media_refs.iter().cloned())
        .collect::<Vec<_>>();
    let mention_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.mention_refs.iter().cloned())
        .collect::<Vec<_>>();
    let hashtag_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.hashtag_refs.iter().cloned())
        .collect::<Vec<_>>();
    let symbol_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.symbol_refs.iter().cloned())
        .collect::<Vec<_>>();
    match store
        .sync_tweet_relations(
            &tweet_ids,
            &media_refs,
            &mention_refs,
            &hashtag_refs,
            &symbol_refs,
        )
        .await
    {
        Ok(statuses) => {
            for item in &prepared.tweet_relations {
                let status = statuses.get(&item.tweet_id).copied();
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_media_ref",
                    status.map(|value| value.media),
                );
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_mention_ref",
                    status.map(|value| value.mention),
                );
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_hashtag_ref",
                    status.map(|value| value.hashtag),
                );
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_symbol_ref",
                    status.map(|value| value.symbol),
                );
            }
        }
        Err(error) => {
            for item in &prepared.tweet_relations {
                prepared.tweet_results[item.index].failed("tweet_media_ref", error.to_string());
                prepared.tweet_results[item.index].failed("tweet_mention_ref", error.to_string());
                prepared.tweet_results[item.index].failed("tweet_hashtag_ref", error.to_string());
                prepared.tweet_results[item.index].failed("tweet_symbol_ref", error.to_string());
            }
        }
    }
}

fn record_conditional_write(
    result: &mut ObjectResultBuilder,
    name: &'static str,
    write: ConditionalWrite,
) {
    match write {
        ConditionalWrite::Inserted => result.accepted(name, "inserted"),
        ConditionalWrite::SkippedDuplicate => result.skipped(name, "duplicate_timestamp"),
        ConditionalWrite::SkippedUnchanged => result.skipped(name, "unchanged"),
        ConditionalWrite::SkippedInterval => result.skipped(name, "stats_interval_not_reached"),
        ConditionalWrite::SkippedMissingParent => result.skipped(name, "missing_parent"),
    }
}

pub(super) fn summarize_results(response: &SubmitTweetResponse) -> SubmitSummary {
    let mut summary = SubmitSummary::default();
    for result in response
        .users
        .iter()
        .chain(response.tweets.iter())
        .chain(response.media.iter())
    {
        summary.total += 1;
        match result.status {
            SubmitObjectStatus::Accepted => summary.accepted += 1,
            SubmitObjectStatus::Skipped => summary.skipped += 1,
            SubmitObjectStatus::Partial => summary.partial += 1,
            SubmitObjectStatus::Failed => summary.failed += 1,
        }
    }
    summary
}
