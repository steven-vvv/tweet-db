use super::*;

pub(super) async fn write_tweet_places_batch(
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

pub(super) async fn write_tweets_batch(
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

pub(super) async fn write_tweet_edits_batch(
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

pub(super) async fn write_tweet_policies_batch(
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

pub(super) async fn write_tweet_community_notes_batch(
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

pub(super) async fn write_tweet_stats_batch(
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
