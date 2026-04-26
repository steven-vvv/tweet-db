use super::*;

pub(in crate::tweet_submit) async fn convert_annotated_text(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    value: &SubmitAnnotatedText,
) -> AppResult<AnnotatedText> {
    let hashtag_models = value
        .entities
        .hashtags
        .iter()
        .map(|entity| Hashtag {
            tag: entity.text.clone(),
        })
        .collect::<Vec<_>>();
    let fallback_hashtag_ids;
    let hashtag_ids = match lookup_ids {
        Some(lookup_ids) => &lookup_ids.hashtags,
        None => {
            fallback_hashtag_ids = store.upsert_hashtags(&hashtag_models).await?;
            &fallback_hashtag_ids
        }
    };

    let symbol_models = value
        .entities
        .symbols
        .iter()
        .map(|entity| Symbol {
            symbol: entity.text.clone(),
            ticker: entity.ticker.clone(),
            name: entity.name.clone(),
        })
        .collect::<Vec<_>>();
    let fallback_symbol_ids;
    let symbol_ids = match lookup_ids {
        Some(lookup_ids) => &lookup_ids.symbols,
        None => {
            fallback_symbol_ids = store.upsert_symbols(&symbol_models).await?;
            &fallback_symbol_ids
        }
    };

    Ok(AnnotatedText {
        body: value.text.clone(),
        display_range_start: value.display_range.map(|range| range.start),
        display_range_end: value.display_range.map(|range| range.end),
        hashtags: value
            .entities
            .hashtags
            .iter()
            .map(|entity| {
                let range = validate_range(entity.range, "text.hashtags.range")?;
                let hashtag_id = hashtag_ids.get(&entity.text).copied().ok_or_else(|| {
                    AppError::bad_request("failed to resolve hashtag dictionary id")
                })?;
                Ok(HashtagRef {
                    hashtag_id,
                    range_start: range.start,
                    range_end: range.end,
                })
            })
            .collect::<AppResult<_>>()?,
        symbols: value
            .entities
            .symbols
            .iter()
            .filter_map(|entity| entity.range.map(|range| (entity, range)))
            .map(|(entity, range)| {
                let range = validate_range(range, "text.symbols.range")?;
                let symbol_id = symbol_ids.get(&entity.text).copied().ok_or_else(|| {
                    AppError::bad_request("failed to resolve symbol dictionary id")
                })?;
                Ok(SymbolRef {
                    symbol_id,
                    range_start: range.start,
                    range_end: range.end,
                })
            })
            .collect::<AppResult<_>>()?,
        urls: value
            .entities
            .urls
            .iter()
            .map(|entity| {
                let range = validate_range(entity.range, "text.urls.range")?;
                Ok(UrlEntity {
                    url: entity.url.clone(),
                    expanded_url: entity.expanded_url.clone(),
                    display_text: entity.display_text.clone(),
                    range_start: range.start,
                    range_end: range.end,
                })
            })
            .collect::<AppResult<_>>()?,
        mentions: value
            .entities
            .mentions
            .iter()
            .map(|entity| {
                let range = validate_range(entity.range, "text.mentions.range")?;
                Ok(MentionEntity {
                    user_id: parse_i64_id(&entity.user_id, "text.mentions.userId")?,
                    range_start: range.start,
                    range_end: range.end,
                })
            })
            .collect::<AppResult<_>>()?,
        media_refs: value
            .entities
            .media
            .iter()
            .map(|entity| {
                let range = entity
                    .range
                    .map(|range| validate_range(range, "text.media.range"))
                    .transpose()?
                    .unwrap_or(SubmitTextRange { start: 0, end: 0 });
                Ok(MediaEntity {
                    media_id: parse_i64_id(&entity.media_id, "text.media.mediaId")?,
                    range_start: range.start,
                    range_end: range.end,
                    display_text: entity.display_text.clone().unwrap_or_default(),
                    expanded_url: entity.expanded_url.clone().unwrap_or_default(),
                    url: entity.url.clone().unwrap_or_default(),
                    origin_tweet_id: entity
                        .origin
                        .as_ref()
                        .and_then(|origin| origin.tweet_id.as_deref())
                        .map(|id| parse_i64_id(id, "text.media.origin.tweetId"))
                        .transpose()?,
                    origin_user_id: entity
                        .origin
                        .as_ref()
                        .and_then(|origin| origin.user_id.as_deref())
                        .map(|id| parse_i64_id(id, "text.media.origin.userId"))
                        .transpose()?,
                })
            })
            .collect::<AppResult<_>>()?,
        styles: value
            .styles
            .iter()
            .map(|style| {
                let range = validate_range(style.range, "text.styles.range")?;
                Ok(TextStyleRange {
                    range_start: range.start,
                    range_end: range.end,
                    styles: style.styles.clone(),
                })
            })
            .collect::<AppResult<_>>()?,
    })
}

pub(in crate::tweet_submit) fn convert_resolved_url(value: &SubmitResolvedUrl) -> ResolvedUrl {
    ResolvedUrl {
        url: value.url.clone(),
        expanded_url: value.expanded_url.clone(),
        display_text: value.display_text.clone(),
    }
}

pub(in crate::tweet_submit) fn collect_text_refs(
    tweet_id: i64,
    text: &AnnotatedText,
    mention_refs: &mut Vec<TweetMentionRef>,
    hashtag_refs: &mut Vec<TweetHashtagRef>,
    symbol_refs: &mut Vec<TweetSymbolRef>,
) {
    mention_refs.extend(text.mentions.iter().map(|reference| TweetMentionRef {
        tweet_id,
        user_id: reference.user_id,
    }));
    hashtag_refs.extend(text.hashtags.iter().map(|reference| TweetHashtagRef {
        tweet_id,
        hashtag_id: reference.hashtag_id,
    }));
    symbol_refs.extend(text.symbols.iter().map(|reference| TweetSymbolRef {
        tweet_id,
        symbol_id: reference.symbol_id,
    }));
}

pub(in crate::tweet_submit) fn dedupe_refs(
    mention_refs: &mut Vec<TweetMentionRef>,
    hashtag_refs: &mut Vec<TweetHashtagRef>,
    symbol_refs: &mut Vec<TweetSymbolRef>,
) {
    let mut mentions = HashSet::new();
    mention_refs.retain(|reference| mentions.insert(reference.user_id));

    let mut hashtags = HashSet::new();
    hashtag_refs.retain(|reference| hashtags.insert(reference.hashtag_id));

    let mut symbols = HashSet::new();
    symbol_refs.retain(|reference| symbols.insert(reference.symbol_id));
}

pub(in crate::tweet_submit) fn dedupe_media_refs(media_refs: &mut Vec<TweetMediaRef>) {
    let mut seen = HashSet::new();
    media_refs.retain(|reference| seen.insert(reference.media_id));
}
