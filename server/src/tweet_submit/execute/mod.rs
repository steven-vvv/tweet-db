use super::*;
use crate::transfer::{self, EnqueueTransferTask, TransferEnqueueStatus};

mod media;
mod relations;
mod tweets;
mod users;

use self::{media::*, relations::*, tweets::*, users::*};

pub(super) async fn execute_prepared_submit(
    state: &AppState,
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
    let media_resource_statuses = write_media_resources_batch(
        store,
        &prepared.media_resources,
        &mut prepared.media_results,
    )
    .await;
    enqueue_prepared_media_transfers(
        state,
        &prepared.media,
        &prepared.media_resources,
        &media_resource_statuses,
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
