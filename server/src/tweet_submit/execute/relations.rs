use super::*;

pub(super) fn record_tweet_write_statuses<T>(
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

pub(super) fn record_relation_sync(
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

pub(super) async fn replace_prepared_tweet_relations(
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

pub(super) fn record_conditional_write(
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
