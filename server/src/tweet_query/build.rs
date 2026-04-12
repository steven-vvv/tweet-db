use super::*;

pub(super) async fn build_user_json(
    user: &DbTwitterUser,
    snapshot: Option<&DbUserSnapshot>,
    stats: Option<&DbUserStats>,
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    categories: &HashMap<i16, UserCategory>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let profile = match snapshot {
        Some(snapshot) => {
            Some(build_user_profile_json(snapshot, hashtags, symbols, string_dict).await?)
        }
        None => None,
    };
    let identity = match snapshot.and_then(|snapshot| snapshot.identity.as_ref()) {
        Some(identity) => Some(build_user_identity_json(identity, string_dict).await?),
        None => None,
    };
    let professional = match snapshot.and_then(|snapshot| snapshot.professional.as_ref()) {
        Some(professional) => {
            Some(build_user_professional_json(professional, categories, string_dict).await?)
        }
        None => None,
    };
    let features = snapshot
        .and_then(|snapshot| snapshot.features.as_ref())
        .map(build_user_features_json);
    let stats = match stats {
        Some(stats) => Some(build_user_stats_json(stats)),
        None => None,
    };

    Ok(json!({
        "id": user.id.to_string(),
        "registeredAt": format_time_opt(user.registered_at),
        "profile": profile,
        "pinnedTweetIds": snapshot
            .map(|snapshot| {
                snapshot
                    .pinned_tweet_ids
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        "identity": identity,
        "professional": professional,
        "stats": stats,
        "features": features,
    }))
}

pub(super) async fn build_user_profile_json(
    snapshot: &DbUserSnapshot,
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let bio = match snapshot.bio.as_ref() {
        Some(bio) => Some(build_annotated_text_json(bio, hashtags, symbols, string_dict).await?),
        None => None,
    };
    let avatar_shape = resolve_optional_string(
        string_dict,
        snapshot.avatar_shape_id,
        StringSemantic::TweetUserAvatarShape,
        "user.profile.avatarShape",
    )
    .await?;

    Ok(json!({
        "fetchedAt": format_time(snapshot.recorded_at),
        "displayName": snapshot.display_name,
        "userName": snapshot.user_name,
        "avatarUrl": snapshot.avatar_url,
        "usesDefaultAvatar": snapshot.uses_default_avatar,
        "avatarShape": avatar_shape,
        "bannerUrl": snapshot.banner_url,
        "location": snapshot.location,
        "bio": bio,
        "profileLinks": snapshot
            .profile_links
            .iter()
            .map(resolved_url_json)
            .collect::<Vec<_>>(),
    }))
}

pub(super) fn build_user_stats_json(stats: &DbUserStats) -> Value {
    json!({
        "fetchedAt": format_time(stats.recorded_at),
        "followers": stats.followers,
        "following": stats.following,
        "likes": stats.likes,
        "mediaPosts": stats.media_posts,
        "tweets": stats.tweets,
        "listed": stats.listed,
    })
}

pub(super) fn build_user_features_json(features: &DbUserFeatures) -> Value {
    json!({
        "canDm": features.can_dm,
        "canTagMedia": features.can_tag_media,
        "isProtected": features.is_protected,
        "canBeSubscribed": features.can_be_subscribed,
    })
}

pub(super) async fn build_user_identity_json(
    identity: &DbUserIdentity,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let verification = match identity.verification.as_ref() {
        Some(verification) => Some(build_user_verification_json(verification, string_dict).await?),
        None => None,
    };
    let disclosure = match identity.disclosure.as_ref() {
        Some(disclosure) => Some(build_user_disclosure_json(disclosure, string_dict).await?),
        None => None,
    };
    let parody_label = resolve_optional_string(
        string_dict,
        identity.parody_label_id,
        StringSemantic::TweetUserParodyLabel,
        "user.identity.parodyLabel",
    )
    .await?;

    Ok(json!({
        "verification": verification,
        "disclosure": disclosure,
        "parodyLabel": parody_label,
        "hasCompletedNewAccountReview": identity.has_completed_new_account_review,
        "isPossiblySensitive": identity.is_possibly_sensitive,
    }))
}

pub(super) async fn build_user_verification_json(
    verification: &DbUserVerification,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let verification_type = resolve_optional_string(
        string_dict,
        verification.verified_type_id,
        StringSemantic::TweetUserVerificationType,
        "user.identity.verification.type",
    )
    .await?;

    Ok(json!({
        "isBlueVerified": verification.is_blue_verified,
        "type": verification_type,
    }))
}

pub(super) async fn build_user_disclosure_json(
    disclosure: &DbUserDisclosure,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let relation = resolve_optional_string(
        string_dict,
        disclosure.relation_id,
        StringSemantic::TweetUserDisclosureRelation,
        "user.identity.disclosure.relation",
    )
    .await?;

    Ok(json!({
        "relation": relation,
        "subjectId": disclosure.subject_id.map(|id| id.to_string()),
        "subjectHandle": disclosure.subject_handle,
        "subjectName": disclosure.subject_name,
        "subjectUrl": disclosure.subject_url,
    }))
}

pub(super) async fn build_user_professional_json(
    professional: &DbUserProfessional,
    categories: &HashMap<i16, UserCategory>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let professional_type = resolve_optional_string(
        string_dict,
        professional.professional_type_id,
        StringSemantic::TweetUserProfessionalType,
        "user.professional.type",
    )
    .await?;
    let categories = professional
        .category_ids
        .iter()
        .map(|category_id| {
            let category = categories
                .get(category_id)
                .ok_or_else(|| format!("missing user category {category_id}"))?;
            Ok(json!({
                "id": category.source_category_code.to_string(),
                "name": category.name,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(json!({
        "id": professional.professional_id.map(|id| id.to_string()),
        "type": professional_type,
        "categories": categories,
    }))
}

pub(super) async fn build_tweet_json(
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

pub(super) async fn build_tweet_place_json(
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

pub(super) async fn build_tweet_content_json(
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

pub(super) fn conversation_json(tweet: &DbTweet) -> Value {
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

pub(super) fn build_tweet_stats_json(stats: &DbTweetStats) -> Value {
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

pub(super) fn build_tweet_edit_json(edit: &DbTweetEdit) -> Value {
    json!({
        "versionIds": edit.version_ids.iter().map(i64::to_string).collect::<Vec<_>>(),
        "editableUntilAt": format_time_opt(edit.editable_until),
        "remainingEdits": edit.remaining_edits.map(|value| value.to_string()),
    })
}

pub(super) async fn build_tweet_policy_json(
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

pub(super) async fn build_tweet_community_note_json(
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

pub(super) async fn build_media_json(
    media: &DbMedia,
    resource: Option<&DbMediaResource>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let variants = match media.size_variants.as_ref() {
        Some(variants) => Some(build_media_variants_json(variants, string_dict).await?),
        None => None,
    };
    let tagged_users = build_media_tags_json(&media.tagged_users, string_dict).await?;
    let sensitivity_warnings = resolve_string_list(
        string_dict,
        &media.sensitivity_warning_ids,
        StringSemantic::TweetMediaSensitivityCode,
        "media.sensitivityWarnings",
    )
    .await?;
    let resource = match resource {
        Some(resource) => Some(build_media_resource_json(resource, string_dict).await?),
        None => None,
    };

    Ok(json!({
        "id": media.id.to_string(),
        "type": media.media_type,
        "altText": media.alt_text,
        "grokPostId": media.grok_post_id.map(|id| id.to_string()),
        "geometry": media.geometry.as_ref().map(media_geometry_json),
        "variants": variants,
        "taggedUsers": tagged_users,
        "sensitivityWarnings": sensitivity_warnings,
        "origin": media_origin_json(media.origin_tweet_id, media.origin_user_id),
        "details": media.details.as_ref().map(media_details_json),
        "resource": resource,
    }))
}

pub(super) async fn build_media_variants_json(
    variants: &DbMediaSizeVariants,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    Ok(json!({
        "large": match variants.large.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "medium": match variants.medium.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "small": match variants.small.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "thumb": match variants.thumb.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
    }))
}

pub(super) async fn build_media_variant_json(
    variant: &DbMediaSizeVariant,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let resize_mode = resolve_optional_string(
        string_dict,
        variant.resize_mode_id,
        StringSemantic::TweetMediaResizeMode,
        "media.variants.resizeMode",
    )
    .await?;

    Ok(json!({
        "width": variant.w,
        "height": variant.h,
        "resizeMode": resize_mode,
    }))
}

pub(super) async fn build_media_tags_json(
    tags: &[DbMediaTag],
    string_dict: &StringDictCache,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::with_capacity(tags.len());
    for tag in tags {
        let kind = resolve_optional_string(
            string_dict,
            tag.kind_id,
            StringSemantic::TweetMediaTagKind,
            "media.taggedUsers.kind",
        )
        .await?;
        values.push(json!({
            "userId": tag.user_id.map(|id| id.to_string()),
            "kind": kind,
        }));
    }
    Ok(values)
}

pub(super) async fn build_media_resource_json(
    resource: &DbMediaResource,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let availability = resolve_optional_string(
        string_dict,
        resource.availability_id,
        StringSemantic::TweetMediaAvailabilityStatus,
        "media.resource.availability",
    )
    .await?;
    let video = match resource.video.as_ref() {
        Some(video) => Some(build_media_video_json(video, string_dict).await?),
        None => None,
    };

    Ok(json!({
        "fetchedAt": format_time(resource.recorded_at),
        "mediaUrl": resource.media_url,
        "availability": availability,
        "video": video,
    }))
}

pub(super) async fn build_media_video_json(
    video: &DbMediaVideo,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let mut variants = Vec::with_capacity(video.variants.len());
    for variant in &video.variants {
        let content_type = resolve_optional_string(
            string_dict,
            variant.content_type_id,
            StringSemantic::TweetVideoContentType,
            "media.video.variants.contentType",
        )
        .await?;
        variants.push(json!({
            "contentType": content_type,
            "bitrate": variant.bitrate,
            "url": variant.url,
        }));
    }

    Ok(json!({
        "aspectRatio": match (video.aspect_ratio_w, video.aspect_ratio_h) {
            (Some(w), Some(h)) => Some([w, h]),
            _ => None::<[i32; 2]>,
        },
        "durationMs": video.duration_ms,
        "variants": variants,
    }))
}

pub(super) async fn build_annotated_text_json(
    text: &DbAnnotatedText,
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let hashtags = text
        .hashtags
        .iter()
        .map(|reference| {
            let hashtag = hashtags
                .get(&reference.hashtag_id)
                .ok_or_else(|| format!("missing hashtag {}", reference.hashtag_id))?;
            Ok(json!({
                "text": hashtag.tag,
                "range": range_json(reference.range_start, reference.range_end),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let symbols = text
        .symbols
        .iter()
        .map(|reference| {
            let symbol = symbols
                .get(&reference.symbol_id)
                .ok_or_else(|| format!("missing symbol {}", reference.symbol_id))?;
            Ok(json!({
                "text": symbol.symbol,
                "range": range_json(reference.range_start, reference.range_end),
                "ticker": symbol.ticker,
                "name": symbol.name,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let urls = text
        .urls
        .iter()
        .map(|entity| {
            json!({
                "url": entity.url,
                "expandedUrl": entity.expanded_url,
                "displayText": entity.display_text,
                "range": range_json(entity.range_start, entity.range_end),
            })
        })
        .collect::<Vec<_>>();
    let mentions = text
        .mentions
        .iter()
        .map(|entity| {
            json!({
                "userId": entity.user_id.to_string(),
                "range": range_json(entity.range_start, entity.range_end),
            })
        })
        .collect::<Vec<_>>();
    let media = text
        .media_refs
        .iter()
        .map(media_entity_json)
        .collect::<Vec<_>>();
    let mut styles = Vec::with_capacity(text.styles.len());
    for style in &text.styles {
        styles.push(json!({
            "range": range_json(style.range_start, style.range_end),
            "styles": resolve_string_list(
                string_dict,
                &style.style_ids,
                StringSemantic::TweetTextStyleName,
                "text.styles",
            )
            .await?,
        }));
    }

    Ok(json!({
        "text": text.body,
        "displayRange": display_range_json(text.display_range_start, text.display_range_end),
        "entities": {
            "hashtags": hashtags,
            "symbols": symbols,
            "urls": urls,
            "mentions": mentions,
            "media": media,
        },
        "styles": styles,
    }))
}

pub(super) fn resolved_url_json(url: &DbResolvedUrl) -> Value {
    json!({
        "url": url.url,
        "expandedUrl": url.expanded_url,
        "displayText": url.display_text,
    })
}

pub(super) fn geo_point_json(point: &DbGeoPoint) -> Value {
    json!({
        "longitude": point.longitude,
        "latitude": point.latitude,
    })
}

pub(super) fn media_geometry_json(geometry: &DbMediaGeometry) -> Value {
    json!({
        "width": geometry.w,
        "height": geometry.h,
        "focusRects": geometry
            .focus_rects
            .iter()
            .map(|rect| {
                json!({
                    "x": rect.x,
                    "y": rect.y,
                    "width": rect.w,
                    "height": rect.h,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub(super) fn media_details_json(details: &DbMediaDetails) -> Value {
    json!({
        "title": details.title,
        "description": details.description,
        "siteUrl": details.site_url,
        "isEmbeddable": details.is_embeddable,
        "isMonetizable": details.is_monetizable,
    })
}

pub(super) fn media_entity_json(entity: &DbMediaEntity) -> Value {
    json!({
        "mediaId": entity.media_id.to_string(),
        "range": range_json(entity.range_start, entity.range_end),
        "displayText": empty_string_as_none(&entity.display_text),
        "expandedUrl": empty_string_as_none(&entity.expanded_url),
        "url": empty_string_as_none(&entity.url),
        "origin": media_origin_json(entity.origin_tweet_id, entity.origin_user_id),
    })
}

pub(super) fn media_origin_json(
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
) -> Option<Value> {
    (origin_tweet_id.is_some() || origin_user_id.is_some()).then(|| {
        json!({
            "tweetId": origin_tweet_id.map(|id| id.to_string()),
            "userId": origin_user_id.map(|id| id.to_string()),
        })
    })
}

pub(super) fn range_json(start: i32, end: i32) -> Value {
    json!({
        "start": start,
        "end": end,
    })
}

pub(super) fn display_range_json(start: Option<i32>, end: Option<i32>) -> Option<Value> {
    match (start, end) {
        (Some(start), Some(end)) => Some(range_json(start, end)),
        _ => None,
    }
}

pub(super) async fn resolve_optional_string(
    string_dict: &StringDictCache,
    id: Option<i16>,
    semantic: StringSemantic,
    field: &str,
) -> Result<Option<String>, String> {
    match id {
        Some(id) => resolve_string(string_dict, id, semantic, field)
            .await
            .map(Some),
        None => Ok(None),
    }
}

pub(super) async fn resolve_string(
    string_dict: &StringDictCache,
    id: i16,
    semantic: StringSemantic,
    field: &str,
) -> Result<String, String> {
    let entry = string_dict
        .get_entry(id)
        .await
        .ok_or_else(|| format!("missing string dictionary entry {id} for {field}"))?;
    ensure_semantic(&entry, semantic, field)?;
    Ok(entry.value)
}

pub(super) async fn resolve_string_list(
    string_dict: &StringDictCache,
    ids: &[i16],
    semantic: StringSemantic,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        values.push(resolve_string(string_dict, *id, semantic, field).await?);
    }
    Ok(values)
}

pub(super) fn ensure_semantic(
    entry: &StringDictValue,
    semantic: StringSemantic,
    field: &str,
) -> Result<(), String> {
    if entry.semantic == semantic {
        Ok(())
    } else {
        Err(format!(
            "dictionary semantic mismatch for {field}: expected {:?}, got {:?}",
            semantic, entry.semantic
        ))
    }
}

pub(super) fn collect_optional_annotated_text_lookup_ids(
    text: Option<&DbAnnotatedText>,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    if let Some(text) = text {
        collect_annotated_text_lookup_ids(text, hashtag_ids, symbol_ids);
    }
}

pub(super) fn collect_annotated_text_lookup_ids(
    text: &DbAnnotatedText,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    hashtag_ids.extend(text.hashtags.iter().map(|reference| reference.hashtag_id));
    symbol_ids.extend(text.symbols.iter().map(|reference| reference.symbol_id));
}
