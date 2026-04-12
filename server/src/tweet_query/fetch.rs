use super::*;

pub(super) async fn query_users(
    store: &TweetStore<'_>,
    string_dict: &StringDictCache,
    selectors: &[QueryIdSelector],
) -> AppResult<Vec<QueryObjectResult>> {
    let selections = parse_i64_selections(selectors, "user.id");
    let user_ids = collect_unique_valid_ids(&selections);
    if user_ids.is_empty() {
        return Ok(selections
            .into_iter()
            .map(selection_failed_or_missing_result)
            .collect());
    }

    let (users_raw, snapshots_raw, stats_raw) = tokio::try_join!(
        store.fetch_users_json_many(&user_ids),
        store.fetch_latest_user_snapshots_json_many(&user_ids),
        store.fetch_latest_user_stats_json_many(&user_ids),
    )?;

    let users = decode_i64_map::<DbTwitterUser>(users_raw, "user");
    let snapshots = decode_i64_map::<DbUserSnapshot>(snapshots_raw, "latest user snapshot");
    let stats = decode_i64_map::<DbUserStats>(stats_raw, "latest user stats");

    let mut hashtag_ids = HashSet::new();
    let mut symbol_ids = HashSet::new();
    let mut category_ids = HashSet::new();
    for snapshot in snapshots.values().filter_map(|item| item.as_ref().ok()) {
        collect_optional_annotated_text_lookup_ids(
            snapshot.bio.as_ref(),
            &mut hashtag_ids,
            &mut symbol_ids,
        );
        if let Some(professional) = snapshot.professional.as_ref() {
            category_ids.extend(professional.category_ids.iter().copied());
        }
    }

    let hashtag_ids = hashtag_ids.into_iter().collect::<Vec<_>>();
    let symbol_ids = symbol_ids.into_iter().collect::<Vec<_>>();
    let category_ids = category_ids.into_iter().collect::<Vec<_>>();
    let (hashtags, symbols, categories) = tokio::try_join!(
        store.fetch_hashtags(&hashtag_ids),
        store.fetch_symbols(&symbol_ids),
        store.fetch_user_categories(&category_ids),
    )?;

    let mut results = Vec::with_capacity(selections.len());
    for selection in selections {
        if let Some(error) = selection.error {
            results.push(failed_result(Some(selection.id), error));
            continue;
        }

        let Some(id) = selection.parsed else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        let Some(user) = decode_required(&users, &id) else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        match (
            user,
            decode_optional(&snapshots, &id),
            decode_optional(&stats, &id),
        ) {
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                results.push(failed_result(Some(selection.id), error))
            }
            (Ok(user), Ok(snapshot), Ok(stats)) => match build_user_json(
                user,
                snapshot,
                stats,
                &hashtags,
                &symbols,
                &categories,
                string_dict,
            )
            .await
            {
                Ok(data) => results.push(found_result(Some(selection.id), data)),
                Err(error) => results.push(failed_result(Some(selection.id), error)),
            },
        }
    }

    Ok(results)
}

pub(super) async fn query_tweet_objects(
    store: &TweetStore<'_>,
    string_dict: &StringDictCache,
    selectors: &[QueryIdSelector],
) -> AppResult<Vec<QueryObjectResult>> {
    let selections = parse_i64_selections(selectors, "tweet.id");
    let tweet_ids = collect_unique_valid_ids(&selections);
    if tweet_ids.is_empty() {
        return Ok(selections
            .into_iter()
            .map(selection_failed_or_missing_result)
            .collect());
    }

    let (tweets_raw, edits_raw, policies_raw, notes_raw, stats_raw, media_refs) = tokio::try_join!(
        store.fetch_tweets_json_many(&tweet_ids),
        store.fetch_tweet_edits_json_many(&tweet_ids),
        store.fetch_tweet_policies_json_many(&tweet_ids),
        store.fetch_tweet_community_notes_json_many(&tweet_ids),
        store.fetch_latest_tweet_stats_json_many(&tweet_ids),
        store.fetch_tweet_media_refs(&tweet_ids),
    )?;

    let tweets = decode_i64_map::<DbTweet>(tweets_raw, "tweet");
    let edits = decode_i64_map::<DbTweetEdit>(edits_raw, "tweet edit");
    let policies = decode_i64_map::<DbTweetPolicy>(policies_raw, "tweet policy");
    let notes = decode_i64_map::<DbTweetCommunityNote>(notes_raw, "tweet community note");
    let stats = decode_i64_map::<DbTweetStats>(stats_raw, "latest tweet stats");

    let place_ids = tweets
        .values()
        .filter_map(|item| item.as_ref().ok())
        .filter_map(|tweet| tweet.place_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut hashtag_ids = HashSet::new();
    let mut symbol_ids = HashSet::new();
    for tweet in tweets.values().filter_map(|item| item.as_ref().ok()) {
        collect_annotated_text_lookup_ids(&tweet.legacy_text, &mut hashtag_ids, &mut symbol_ids);
        collect_optional_annotated_text_lookup_ids(
            tweet.note_text.as_ref(),
            &mut hashtag_ids,
            &mut symbol_ids,
        );
    }
    for note in notes.values().filter_map(|item| item.as_ref().ok()) {
        collect_optional_annotated_text_lookup_ids(
            note.subtitle.as_ref(),
            &mut hashtag_ids,
            &mut symbol_ids,
        );
        collect_optional_annotated_text_lookup_ids(
            note.footer.as_ref(),
            &mut hashtag_ids,
            &mut symbol_ids,
        );
    }

    let hashtag_ids = hashtag_ids.into_iter().collect::<Vec<_>>();
    let symbol_ids = symbol_ids.into_iter().collect::<Vec<_>>();
    let (places_raw, hashtags, symbols) = tokio::try_join!(
        store.fetch_tweet_places_json_many(&place_ids),
        store.fetch_hashtags(&hashtag_ids),
        store.fetch_symbols(&symbol_ids),
    )?;
    let places = decode_string_map::<DbTweetPlace>(places_raw, "tweet place");

    let mut results = Vec::with_capacity(selections.len());
    for selection in selections {
        if let Some(error) = selection.error {
            results.push(failed_result(Some(selection.id), error));
            continue;
        }

        let Some(id) = selection.parsed else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        let Some(tweet) = decode_required(&tweets, &id) else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        match (
            tweet,
            decode_optional(&edits, &id),
            decode_optional(&policies, &id),
            decode_optional(&notes, &id),
            decode_optional(&stats, &id),
        ) {
            (Err(error), _, _, _, _)
            | (_, Err(error), _, _, _)
            | (_, _, Err(error), _, _)
            | (_, _, _, Err(error), _)
            | (_, _, _, _, Err(error)) => results.push(failed_result(Some(selection.id), error)),
            (Ok(tweet), Ok(edit), Ok(policy), Ok(note), Ok(stats)) => {
                let place = match tweet.place_id.as_ref() {
                    Some(place_id) => match decode_optional(&places, place_id) {
                        Ok(place) => place,
                        Err(error) => {
                            results.push(failed_result(Some(selection.id), error));
                            continue;
                        }
                    },
                    None => None,
                };

                let refs = media_refs.get(&id).cloned().unwrap_or_default();
                match build_tweet_json(
                    tweet,
                    place,
                    edit,
                    policy,
                    note,
                    stats,
                    &refs,
                    &hashtags,
                    &symbols,
                    string_dict,
                )
                .await
                {
                    Ok(data) => results.push(found_result(Some(selection.id), data)),
                    Err(error) => results.push(failed_result(Some(selection.id), error)),
                }
            }
        }
    }

    Ok(results)
}

pub(super) async fn query_media_objects(
    store: &TweetStore<'_>,
    string_dict: &StringDictCache,
    selectors: &[QueryIdSelector],
) -> AppResult<Vec<QueryObjectResult>> {
    let selections = parse_i64_selections(selectors, "media.id");
    let media_ids = collect_unique_valid_ids(&selections);
    if media_ids.is_empty() {
        return Ok(selections
            .into_iter()
            .map(selection_failed_or_missing_result)
            .collect());
    }

    let (media_raw, resources_raw) = tokio::try_join!(
        store.fetch_media_json_many(&media_ids),
        store.fetch_latest_media_resource_json_many(&media_ids),
    )?;

    let media = decode_i64_map::<DbMedia>(media_raw, "media");
    let resources = decode_i64_map::<DbMediaResource>(resources_raw, "latest media resource");

    let mut results = Vec::with_capacity(selections.len());
    for selection in selections {
        if let Some(error) = selection.error {
            results.push(failed_result(Some(selection.id), error));
            continue;
        }

        let Some(id) = selection.parsed else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        let Some(media) = decode_required(&media, &id) else {
            results.push(missing_result(Some(selection.id)));
            continue;
        };

        match (media, decode_optional(&resources, &id)) {
            (Err(error), _) | (_, Err(error)) => {
                results.push(failed_result(Some(selection.id), error))
            }
            (Ok(media), Ok(resource)) => {
                match build_media_json(media, resource, string_dict).await {
                    Ok(data) => results.push(found_result(Some(selection.id), data)),
                    Err(error) => results.push(failed_result(Some(selection.id), error)),
                }
            }
        }
    }

    Ok(results)
}
