use super::*;

pub(in crate::tweet_query) async fn build_tweet_json(
    tweet: &DbTweet,
    place: Option<&DbTweetPlace>,
    edit: Option<&DbTweetEdit>,
    policy: Option<&DbTweetPolicy>,
    note: Option<&DbTweetCommunityNote>,
    stats: Option<&DbTweetStats>,
    media_refs: &[TweetMediaRef],
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let source = resolve_optional_string(
        string_dict,
        tweet.source_id,
        StringSemantic::TweetSource,
        "tweet.source",
    )
    .await?;
    let place = match place {
        Some(place) => Some(build_tweet_place_json(place, string_dict).await?),
        None => None,
    };
    let content =
        build_tweet_content_json(tweet, media_refs, hashtags, symbols, string_dict).await?;
    let conversation = conversation_json(tweet);
    let stats = stats.map(build_tweet_stats_json);
    let edit = edit.map(build_tweet_edit_json);
    let policy = match policy {
        Some(policy) => Some(build_tweet_policy_json(policy, string_dict).await?),
        None => None,
    };
    let community_note = match note {
        Some(note) => {
            Some(build_tweet_community_note_json(note, hashtags, symbols, string_dict).await?)
        }
        None => None,
    };

    Ok(json!({
        "id": tweet.id.to_string(),
        "publishedAt": format_time(tweet.published_at),
        "source": source,
        "authorId": tweet.author_id.to_string(),
        "place": place,
        "content": content,
        "conversation": conversation,
        "stats": stats,
        "edit": edit,
        "policy": policy,
        "communityNote": community_note,
    }))
}

pub(in crate::tweet_query) async fn build_tweet_place_json(
    place: &DbTweetPlace,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let country = resolve_optional_string(
        string_dict,
        place.country_id,
        StringSemantic::TweetCountryName,
        "tweet.place.country",
    )
    .await?;
    let country_code = resolve_optional_string(
        string_dict,
        place.country_code_id,
        StringSemantic::TweetCountryCode,
        "tweet.place.countryCode",
    )
    .await?;
    let kind = resolve_optional_string(
        string_dict,
        place.kind_id,
        StringSemantic::TweetPlaceKind,
        "tweet.place.kind",
    )
    .await?;

    Ok(json!({
        "id": place.id,
        "name": place.name,
        "fullName": place.full_name,
        "country": country,
        "countryCode": country_code,
        "kind": kind,
        "boundary": place.boundary.as_ref().map(|points| {
            points.iter().map(geo_point_json).collect::<Vec<_>>()
        }),
    }))
}

pub(in crate::tweet_query) async fn build_tweet_content_json(
    tweet: &DbTweet,
    media_refs: &[TweetMediaRef],
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let note = match (&tweet.note_id, tweet.note_text.as_ref()) {
        (None, None) => None,
        (note_id, note_text) => Some(json!({
            "id": note_id,
            "text": match note_text {
                Some(text) => Some(build_annotated_text_json(text, hashtags, symbols, string_dict).await?),
                None => None,
            },
        })),
    };
    let language = resolve_optional_string(
        string_dict,
        tweet.language_id,
        StringSemantic::TweetLanguageCode,
        "tweet.content.language",
    )
    .await?;

    Ok(json!({
        "legacyText": build_annotated_text_json(&tweet.legacy_text, hashtags, symbols, string_dict).await?,
        "note": note,
        "mediaIds": media_refs.iter().map(|reference| reference.media_id.to_string()).collect::<Vec<_>>(),
        "language": language,
    }))
}

pub(in crate::tweet_query) fn conversation_json(tweet: &DbTweet) -> Value {
    json!({
        "conversationId": tweet.conversation_id.to_string(),
        "replyTo": tweet.reply_to_tweet_id.map(|tweet_id| {
            json!({
                "tweetId": tweet_id.to_string(),
                "userId": tweet.reply_to_user_id.map(|user_id| user_id.to_string()),
            })
        }),
        "quote": tweet.quote_tweet_id.map(|tweet_id| {
            json!({
                "tweetId": tweet_id.to_string(),
                "permalink": tweet.quote_permalink.as_ref().map(resolved_url_json),
            })
        }),
        "repostId": tweet.repost_id.map(|id| id.to_string()),
    })
}

pub(in crate::tweet_query) fn build_tweet_stats_json(stats: &DbTweetStats) -> Value {
    json!({
        "fetchedAt": format_time(stats.recorded_at),
        "views": stats.views.map(|value| value.to_string()),
        "replies": stats.replies,
        "reposts": stats.reposts,
        "quotes": stats.quotes,
        "likes": stats.likes,
        "bookmarks": stats.bookmarks,
    })
}

pub(in crate::tweet_query) fn build_tweet_edit_json(edit: &DbTweetEdit) -> Value {
    json!({
        "versionIds": edit.version_ids.iter().map(i64::to_string).collect::<Vec<_>>(),
        "editableUntilAt": format_time_opt(edit.editable_until),
        "remainingEdits": edit.remaining_edits.map(|value| value.to_string()),
    })
}

pub(in crate::tweet_query) async fn build_tweet_policy_json(
    policy: &DbTweetPolicy,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let reply_policy = resolve_optional_string(
        string_dict,
        policy.reply_policy_id,
        StringSemantic::TweetReplyPolicyCode,
        "tweet.policy.replyPolicy",
    )
    .await?;
    let available_actions = resolve_string_list(
        string_dict,
        &policy.available_action_ids,
        StringSemantic::TweetActionCode,
        "tweet.policy.availableActions",
    )
    .await?;

    Ok(json!({
        "replyPolicy": reply_policy,
        "followersOnly": policy.followers_only,
        "isPossiblySensitive": policy.is_possibly_sensitive,
        "availableActions": available_actions,
        "isMediaVisibilityRestricted": policy.is_media_visibility_restricted,
        "paidPromotion": policy.paid_promotion,
    }))
}

pub(in crate::tweet_query) async fn build_tweet_community_note_json(
    note: &DbTweetCommunityNote,
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let subtitle = match note.subtitle.as_ref() {
        Some(text) => Some(build_annotated_text_json(text, hashtags, symbols, string_dict).await?),
        None => None,
    };
    let footer = match note.footer.as_ref() {
        Some(text) => Some(build_annotated_text_json(text, hashtags, symbols, string_dict).await?),
        None => None,
    };

    Ok(json!({
        "id": note.note_id.map(|id| id.to_string()),
        "title": note.title,
        "shortTitle": note.short_title,
        "subtitle": subtitle,
        "footer": footer,
        "destinationUrl": note.destination_url,
    }))
}
