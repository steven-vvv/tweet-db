use super::*;

async fn resolve_submit_lookup_ids(
    store: &TweetStore<'_>,
    payload: &SubmitTweetRequest,
) -> AppResult<SubmitLookupIds> {
    let mut categories = HashMap::<i32, UserCategory>::new();
    let mut hashtags = HashMap::<String, Hashtag>::new();
    let mut symbols = HashMap::<String, Symbol>::new();

    for user in &payload.users {
        if parse_i64_id(&user.id, "user.id").is_err() {
            continue;
        }
        if let Some(professional) = user.professional.as_ref() {
            for category in &professional.categories {
                if let Ok(source_category_code) =
                    parse_i32_id(&category.id, "user.professional.categories.id")
                {
                    categories
                        .entry(source_category_code)
                        .and_modify(|existing| {
                            if existing.name.is_empty() && !category.name.is_empty() {
                                existing.name = category.name.clone();
                            }
                        })
                        .or_insert_with(|| UserCategory {
                            source_category_code,
                            name: category.name.clone(),
                        });
                }
            }
        }

        if let Some(profile) = user.profile.as_ref()
            && let Some(bio) = profile.bio.as_ref()
        {
            collect_lookup_text(bio, &mut hashtags, &mut symbols);
        }
    }

    for tweet in &payload.tweets {
        if parse_i64_id(&tweet.id, "tweet.id").is_err()
            || parse_i64_id(&tweet.author_id, "tweet.authorId").is_err()
        {
            continue;
        }
        collect_lookup_text(&tweet.content.legacy_text, &mut hashtags, &mut symbols);
        if let Some(note) = tweet.content.note.as_ref() {
            collect_lookup_text(&note.text, &mut hashtags, &mut symbols);
        }
        if let Some(community_note) = tweet.community_note.as_ref() {
            if let Some(subtitle) = community_note.subtitle.as_ref() {
                collect_lookup_text(subtitle, &mut hashtags, &mut symbols);
            }
            if let Some(footer) = community_note.footer.as_ref() {
                collect_lookup_text(footer, &mut hashtags, &mut symbols);
            }
        }
    }

    let category_values = categories.into_values().collect::<Vec<_>>();
    let hashtag_values = hashtags.into_values().collect::<Vec<_>>();
    let symbol_values = symbols.into_values().collect::<Vec<_>>();

    Ok(SubmitLookupIds {
        user_categories: store.upsert_user_categories(&category_values).await?,
        hashtags: store.upsert_hashtags(&hashtag_values).await?,
        symbols: store.upsert_symbols(&symbol_values).await?,
    })
}

fn collect_lookup_text(
    text: &SubmitAnnotatedText,
    hashtags: &mut HashMap<String, Hashtag>,
    symbols: &mut HashMap<String, Symbol>,
) {
    for entity in &text.entities.hashtags {
        hashtags
            .entry(entity.text.clone())
            .or_insert_with(|| Hashtag {
                tag: entity.text.clone(),
            });
    }

    for entity in &text.entities.symbols {
        symbols
            .entry(entity.text.clone())
            .and_modify(|existing| {
                if existing.ticker.is_none() && entity.ticker.is_some() {
                    existing.ticker = entity.ticker.clone();
                }
                if existing.name.is_none() && entity.name.is_some() {
                    existing.name = entity.name.clone();
                }
            })
            .or_insert_with(|| Symbol {
                symbol: entity.text.clone(),
                ticker: entity.ticker.clone(),
                name: entity.name.clone(),
            });
    }
}

pub(super) async fn prepare_submit_batch(
    store: &TweetStore<'_>,
    payload: SubmitTweetRequest,
) -> PreparedSubmitBatch {
    let lookup_ids = resolve_submit_lookup_ids(store, &payload).await.ok();
    let mut prepared = PreparedSubmitBatch::new(
        payload.users.len(),
        payload.tweets.len(),
        payload.media.len(),
    );
    let last_user_indices =
        collect_last_valid_i64_indices(&payload.users, "user.id", |user| &user.id);
    let last_tweet_indices =
        collect_last_valid_i64_indices(&payload.tweets, "tweet.id", |tweet| &tweet.id);
    let last_media_indices =
        collect_last_valid_i64_indices(&payload.media, "media.id", |media| &media.id);

    for (index, user) in payload.users.into_iter().enumerate() {
        let now = OffsetDateTime::now_utc();
        let mut result = ObjectResultBuilder::new(Some(user.id.clone()));
        let user_id = match parse_i64_id(&user.id, "user.id") {
            Ok(id) => id,
            Err(error) => {
                result.fatal(error.to_string());
                prepared.user_results.push(result);
                continue;
            }
        };
        if last_user_indices.get(&user_id).copied() != Some(index) {
            result.skipped("input", "shadowed_by_duplicate_input");
            prepared.user_results.push(result);
            continue;
        }

        prepared.users.push(Indexed {
            index,
            value: TwitterUser {
                id: user_id,
                registered_at: user.registered_at,
            },
        });

        if let Some(profile) = user.profile.as_ref() {
            match convert_user_snapshot(store, lookup_ids.as_ref(), user_id, &user, profile, now)
                .await
            {
                Ok(snapshot) => prepared.user_snapshots.push(Indexed {
                    index,
                    value: snapshot,
                }),
                Err(error) => result.failed("user_snapshot", error.to_string()),
            }
        }

        if let Some(stats) = user.stats.as_ref() {
            match convert_user_stats(user_id, user.fetched_at, stats, now) {
                Ok(stats) => prepared.user_stats.push(Indexed {
                    index,
                    value: stats,
                }),
                Err(error) => result.failed("user_stats", error.to_string()),
            }
        }

        prepared.user_results.push(result);
    }

    for (index, media) in payload.media.into_iter().enumerate() {
        let now = OffsetDateTime::now_utc();
        let mut result = ObjectResultBuilder::new(Some(media.id.clone()));
        let media_id = match parse_i64_id(&media.id, "media.id") {
            Ok(id) => id,
            Err(error) => {
                result.fatal(error.to_string());
                prepared.media_results.push(result);
                continue;
            }
        };
        if last_media_indices.get(&media_id).copied() != Some(index) {
            result.skipped("input", "shadowed_by_duplicate_input");
            prepared.media_results.push(result);
            continue;
        }

        match convert_media(media_id, &media) {
            Ok(model) => prepared.media.push(Indexed {
                index,
                value: model,
            }),
            Err(error) => result.failed("media", error.to_string()),
        }

        if let Some(resource) = convert_media_resource(media_id, &media, now) {
            match resource {
                Ok(resource) => prepared.media_resources.push(Indexed {
                    index,
                    value: resource,
                }),
                Err(error) => result.failed("media_resource", error.to_string()),
            }
        }

        prepared.media_results.push(result);
    }

    for (index, tweet) in payload.tweets.into_iter().enumerate() {
        let now = OffsetDateTime::now_utc();
        let mut result = ObjectResultBuilder::new(Some(tweet.id.clone()));
        let tweet_id = match parse_i64_id(&tweet.id, "tweet.id") {
            Ok(id) => id,
            Err(error) => {
                result.fatal(error.to_string());
                prepared.tweet_results.push(result);
                continue;
            }
        };
        if last_tweet_indices.get(&tweet_id).copied() != Some(index) {
            result.skipped("input", "shadowed_by_duplicate_input");
            prepared.tweet_results.push(result);
            continue;
        }
        let author_id = match parse_i64_id(&tweet.author_id, "tweet.authorId") {
            Ok(id) => id,
            Err(error) => {
                result.fatal(error.to_string());
                prepared.tweet_results.push(result);
                continue;
            }
        };

        prepared.tweet_authors.push(Indexed {
            index,
            value: TwitterUser {
                id: author_id,
                registered_at: None,
            },
        });

        let place_id = if let Some(place) = tweet.place.as_ref() {
            match convert_tweet_place(place) {
                Ok(Some(model)) => {
                    let place_id = model.id.clone();
                    prepared.tweet_places.push(Indexed {
                        index,
                        value: model,
                    });
                    Some(place_id)
                }
                Ok(None) => None,
                Err(error) => {
                    result.failed("tweet_place", error.to_string());
                    None
                }
            }
        } else {
            None
        };

        let converted = match convert_tweet(
            store,
            lookup_ids.as_ref(),
            tweet_id,
            author_id,
            place_id,
            &tweet,
            now,
        )
        .await
        {
            Ok(converted) => converted,
            Err(error) => {
                result.failed("tweet", error.to_string());
                prepared.tweet_results.push(result);
                continue;
            }
        };

        prepared.tweets.push(Indexed {
            index,
            value: converted.tweet.clone(),
        });
        if let Some(edit) = converted.edit.clone() {
            prepared.tweet_edits.push(Indexed { index, value: edit });
        }
        if let Some(policy) = converted.policy.clone() {
            prepared.tweet_policies.push(Indexed {
                index,
                value: policy,
            });
        }
        if let Some(note) = converted.community_note.clone() {
            prepared
                .tweet_community_notes
                .push(Indexed { index, value: note });
        }
        if let Some(stats) = converted.stats.clone() {
            prepared.tweet_stats.push(Indexed {
                index,
                value: stats,
            });
        }
        prepared.tweet_relations.push(IndexedTweetRelations {
            index,
            tweet_id,
            media_refs: converted.media_refs,
            mention_refs: converted.mention_refs,
            hashtag_refs: converted.hashtag_refs,
            symbol_refs: converted.symbol_refs,
        });

        prepared.tweet_results.push(result);
    }

    prepared
}
