use super::*;

pub(in crate::tweet_submit) async fn convert_tweet(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    tweet_id: i64,
    author_id: i64,
    place_id: Option<String>,
    value: &SubmitTweet,
    now: OffsetDateTime,
) -> AppResult<ConvertedTweet> {
    let legacy_text = convert_annotated_text(store, lookup_ids, &value.content.legacy_text).await?;
    let note_text = match value.content.note.as_ref() {
        Some(note) => Some(convert_annotated_text(store, lookup_ids, &note.text).await?),
        None => None,
    };
    let note_id = value.content.note.as_ref().and_then(|note| note.id.clone());
    let conversation_id =
        parse_i64_id(&value.conversation.conversation_id, "tweet.conversationId")?;
    let reply_to_tweet_id = value
        .conversation
        .reply_to
        .as_ref()
        .map(|reply| parse_i64_id(&reply.tweet_id, "tweet.replyTo.tweetId"))
        .transpose()?;
    let reply_to_user_id = value
        .conversation
        .reply_to
        .as_ref()
        .and_then(|reply| reply.user_id.as_deref())
        .map(|id| parse_i64_id(id, "tweet.replyTo.userId"))
        .transpose()?;
    let quote_tweet_id = value
        .conversation
        .quote
        .as_ref()
        .map(|quote| parse_i64_id(&quote.tweet_id, "tweet.quote.tweetId"))
        .transpose()?;
    let repost_id = value
        .conversation
        .repost_id
        .as_deref()
        .map(|id| parse_i64_id(id, "tweet.repostId"))
        .transpose()?;
    let quote_permalink = value
        .conversation
        .quote
        .as_ref()
        .and_then(|quote| quote.permalink.as_ref())
        .map(convert_resolved_url);

    let tweet = Tweet {
        id: tweet_id,
        published_at: value.published_at,
        source: value.source.clone(),
        author_id,
        place_id,
        legacy_text: legacy_text.clone(),
        note_id,
        note_text: note_text.clone(),
        language: value.content.language.clone(),
        conversation_id,
        reply_to_tweet_id,
        reply_to_user_id,
        quote_tweet_id,
        quote_permalink,
        repost_id,
    };

    let edit = value
        .edit
        .as_ref()
        .map(|edit| convert_tweet_edit(tweet_id, edit))
        .transpose()?;
    let policy = value
        .policy
        .as_ref()
        .map(|policy| convert_tweet_policy(tweet_id, policy));
    let stats = value
        .stats
        .as_ref()
        .map(|stats| convert_tweet_stats(tweet_id, value.fetched_at, stats, now))
        .transpose()?;
    let community_note = match value.community_note.as_ref() {
        Some(note) => Some(convert_tweet_community_note(store, lookup_ids, tweet_id, note).await?),
        None => None,
    };

    let mut media_refs = Vec::with_capacity(value.content.media_ids.len());
    for (index, media_id) in value.content.media_ids.iter().enumerate() {
        media_refs.push(TweetMediaRef {
            tweet_id,
            media_id: parse_i64_id(media_id, "tweet.content.mediaIds")?,
            display_order: index
                .try_into()
                .map_err(|_| AppError::bad_request("tweet.content.mediaIds is too large"))?,
        });
    }
    dedupe_media_refs(&mut media_refs);
    let mut mention_refs = Vec::new();
    let mut hashtag_refs = Vec::new();
    let mut symbol_refs = Vec::new();
    collect_text_refs(
        tweet_id,
        &legacy_text,
        &mut mention_refs,
        &mut hashtag_refs,
        &mut symbol_refs,
    );
    if let Some(note_text) = note_text.as_ref() {
        collect_text_refs(
            tweet_id,
            note_text,
            &mut mention_refs,
            &mut hashtag_refs,
            &mut symbol_refs,
        );
    }
    dedupe_refs(&mut mention_refs, &mut hashtag_refs, &mut symbol_refs);

    Ok(ConvertedTweet {
        tweet,
        edit,
        policy,
        stats,
        community_note,
        media_refs,
        mention_refs,
        hashtag_refs,
        symbol_refs,
    })
}

pub(in crate::tweet_submit) fn convert_tweet_edit(
    tweet_id: i64,
    edit: &SubmitTweetEdit,
) -> AppResult<TweetEdit> {
    Ok(TweetEdit {
        tweet_id,
        version_ids: parse_i64_ids(&edit.version_ids, "tweet.edit.versionIds")?,
        editable_until: edit.editable_until_at,
        remaining_edits: parse_optional_i32_string(
            edit.remaining_edits.as_deref(),
            "tweet.edit.remainingEdits",
        )?,
    })
}

pub(in crate::tweet_submit) fn convert_tweet_policy(
    tweet_id: i64,
    policy: &SubmitTweetPolicy,
) -> TweetPolicy {
    TweetPolicy {
        tweet_id,
        reply_policy: policy.reply_policy.clone(),
        followers_only: policy.followers_only,
        is_possibly_sensitive: policy.is_possibly_sensitive,
        available_actions: policy.available_actions.clone(),
        is_media_visibility_restricted: policy.is_media_visibility_restricted,
        paid_promotion: policy.paid_promotion,
    }
}

pub(in crate::tweet_submit) fn convert_tweet_stats(
    tweet_id: i64,
    parent_fetched_at: Option<OffsetDateTime>,
    stats: &SubmitTweetStats,
    now: OffsetDateTime,
) -> AppResult<TweetStats> {
    ensure_nonnegative_options(
        "tweet.stats",
        &[
            stats.replies,
            stats.reposts,
            stats.quotes,
            stats.likes,
            stats.bookmarks,
        ],
    )?;

    Ok(TweetStats {
        tweet_id,
        recorded_at: stats.fetched_at.or(parent_fetched_at).unwrap_or(now),
        views: parse_optional_i64_string(stats.views.as_deref(), "tweet.stats.views")?,
        replies: stats.replies,
        reposts: stats.reposts,
        quotes: stats.quotes,
        likes: stats.likes,
        bookmarks: stats.bookmarks,
    })
}

pub(in crate::tweet_submit) async fn convert_tweet_community_note(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    tweet_id: i64,
    note: &SubmitTweetCommunityNote,
) -> AppResult<TweetCommunityNote> {
    Ok(TweetCommunityNote {
        tweet_id,
        note_id: parse_optional_i64_id(note.id.as_deref(), "tweet.communityNote.id")?,
        title: note.title.clone(),
        short_title: note.short_title.clone(),
        subtitle: match note.subtitle.as_ref() {
            Some(text) => Some(convert_annotated_text(store, lookup_ids, text).await?),
            None => None,
        },
        footer: match note.footer.as_ref() {
            Some(text) => Some(convert_annotated_text(store, lookup_ids, text).await?),
            None => None,
        },
        destination_url: note.destination_url.clone(),
    })
}

pub(in crate::tweet_submit) fn convert_tweet_place(
    place: &SubmitTweetPlace,
) -> AppResult<Option<TweetPlace>> {
    let Some(id) = place.id.as_ref() else {
        return Ok(None);
    };

    Ok(Some(TweetPlace {
        id: id.clone(),
        name: place.name.clone(),
        full_name: place.full_name.clone(),
        country: place.country.clone(),
        country_code: place.country_code.clone(),
        kind: place.kind.clone(),
        boundary: place.boundary.as_ref().map(|points| {
            points
                .iter()
                .map(|point| GeoPoint {
                    longitude: point.longitude,
                    latitude: point.latitude,
                })
                .collect()
        }),
    }))
}
