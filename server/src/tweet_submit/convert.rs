use super::*;

pub(super) async fn convert_user_snapshot(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    user_id: i64,
    user: &SubmitUser,
    profile: &SubmitUserProfile,
    now: OffsetDateTime,
) -> AppResult<UserSnapshot> {
    Ok(UserSnapshot {
        user_id,
        recorded_at: profile.fetched_at.or(user.fetched_at).unwrap_or(now),
        display_name: profile.display_name.clone(),
        user_name: profile.user_name.clone(),
        avatar_url: profile.avatar_url.clone(),
        uses_default_avatar: profile.uses_default_avatar,
        avatar_shape: profile.avatar_shape.clone(),
        banner_url: profile.banner_url.clone(),
        location: profile.location.clone(),
        bio: match profile.bio.as_ref() {
            Some(text) => Some(convert_annotated_text(store, lookup_ids, text).await?),
            None => None,
        },
        profile_links: profile
            .profile_links
            .iter()
            .map(convert_resolved_url)
            .collect(),
        identity: convert_user_identity(user.identity.as_ref())?,
        features: user.features.as_ref().map(convert_user_features),
        professional: match user.professional.as_ref() {
            Some(professional) => {
                Some(convert_user_professional(store, lookup_ids, professional).await?)
            }
            None => None,
        },
        pinned_tweet_ids: parse_i64_ids(&user.pinned_tweet_ids, "user.pinnedTweetIds")?,
    })
}

pub(super) fn convert_user_stats(
    user_id: i64,
    parent_fetched_at: Option<OffsetDateTime>,
    stats: &SubmitUserStats,
    now: OffsetDateTime,
) -> AppResult<UserStats> {
    ensure_nonnegative_options(
        "user.stats",
        &[
            stats.followers,
            stats.following,
            stats.likes,
            stats.media_posts,
            stats.tweets,
            stats.listed,
        ],
    )?;

    Ok(UserStats {
        user_id,
        recorded_at: stats.fetched_at.or(parent_fetched_at).unwrap_or(now),
        followers: stats.followers,
        following: stats.following,
        likes: stats.likes,
        media_posts: stats.media_posts,
        tweets: stats.tweets,
        listed: stats.listed,
    })
}

pub(super) async fn convert_user_professional(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    professional: &SubmitUserProfessional,
) -> AppResult<UserProfessional> {
    let categories = professional
        .categories
        .iter()
        .map(|category| {
            Ok(UserCategory {
                source_category_code: parse_i32_id(
                    &category.id,
                    "user.professional.categories.id",
                )?,
                name: category.name.clone(),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let category_ids = match lookup_ids {
        Some(lookup_ids) => categories
            .iter()
            .map(|category| {
                lookup_ids
                    .user_categories
                    .get(&category.source_category_code)
                    .copied()
                    .ok_or_else(|| AppError::bad_request("failed to resolve user category id"))
            })
            .collect::<AppResult<Vec<_>>>()?,
        None => {
            let resolved = store.upsert_user_categories(&categories).await?;
            categories
                .iter()
                .map(|category| {
                    resolved
                        .get(&category.source_category_code)
                        .copied()
                        .ok_or_else(|| AppError::bad_request("failed to resolve user category id"))
                })
                .collect::<AppResult<Vec<_>>>()?
        }
    };

    Ok(UserProfessional {
        professional_id: parse_optional_i64_id(professional.id.as_deref(), "user.professional.id")?,
        professional_type: professional.professional_type.clone(),
        category_ids,
    })
}

pub(super) fn convert_user_identity(
    identity: Option<&SubmitUserIdentity>,
) -> AppResult<Option<UserIdentity>> {
    identity
        .map(|identity| {
            Ok(UserIdentity {
                verification: identity
                    .verification
                    .as_ref()
                    .map(|verification| UserVerification {
                        is_blue_verified: verification.is_blue_verified,
                        verified_type: verification.verification_type.clone(),
                    }),
                disclosure: match identity.disclosure.as_ref() {
                    Some(disclosure) => Some(UserDisclosure {
                        relation: disclosure.relation.clone(),
                        subject_id: parse_optional_i64_id(
                            disclosure.subject_id.as_deref(),
                            "user.identity.disclosure.subjectId",
                        )?,
                        subject_handle: disclosure.subject_handle.clone(),
                        subject_name: disclosure.subject_name.clone(),
                        subject_url: disclosure.subject_url.clone(),
                    }),
                    None => None,
                },
                parody_label: identity.parody_label.clone(),
                has_completed_new_account_review: identity.has_completed_new_account_review,
                is_possibly_sensitive: identity.is_possibly_sensitive,
            })
        })
        .transpose()
}

pub(super) fn convert_user_features(features: &SubmitUserFeatures) -> UserFeatures {
    UserFeatures {
        can_dm: features.can_dm,
        can_tag_media: features.can_tag_media,
        is_protected: features.is_protected,
        can_be_subscribed: features.can_be_subscribed,
    }
}

pub(super) async fn convert_tweet(
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

pub(super) fn convert_tweet_edit(tweet_id: i64, edit: &SubmitTweetEdit) -> AppResult<TweetEdit> {
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

pub(super) fn convert_tweet_policy(tweet_id: i64, policy: &SubmitTweetPolicy) -> TweetPolicy {
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

pub(super) fn convert_tweet_stats(
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

pub(super) async fn convert_tweet_community_note(
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

pub(super) fn convert_tweet_place(place: &SubmitTweetPlace) -> AppResult<Option<TweetPlace>> {
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

pub(super) fn convert_media(media_id: i64, media: &SubmitMedia) -> AppResult<Media> {
    Ok(Media {
        id: media_id,
        media_type: match media.media_type {
            SubmitMediaType::Photo => MediaType::Photo,
            SubmitMediaType::Video => MediaType::Video,
            SubmitMediaType::AnimatedGif => MediaType::AnimatedGif,
        },
        alt_text: media.alt_text.clone(),
        grok_post_id: media
            .grok_post_id
            .as_deref()
            .map(|value| {
                Uuid::parse_str(value)
                    .map_err(|_| AppError::bad_request("media.grokPostId must be a UUID"))
            })
            .transpose()?,
        geometry: media
            .geometry
            .as_ref()
            .map(convert_media_geometry)
            .transpose()?,
        size_variants: media
            .variants
            .as_ref()
            .map(convert_media_variants)
            .transpose()?,
        tagged_users: media
            .tagged_users
            .iter()
            .map(convert_media_tag)
            .collect::<AppResult<_>>()?,
        sensitivity_warnings: media.sensitivity_warnings.clone(),
        origin_tweet_id: media
            .origin
            .as_ref()
            .and_then(|origin| origin.tweet_id.as_deref())
            .map(|id| parse_i64_id(id, "media.origin.tweetId"))
            .transpose()?,
        origin_user_id: media
            .origin
            .as_ref()
            .and_then(|origin| origin.user_id.as_deref())
            .map(|id| parse_i64_id(id, "media.origin.userId"))
            .transpose()?,
        details: media.details.as_ref().map(convert_media_details),
    })
}

pub(super) fn convert_media_resource(
    media_id: i64,
    media: &SubmitMedia,
    now: OffsetDateTime,
) -> Option<AppResult<MediaResource>> {
    let resource = media.resource.as_ref();
    let media_url = resource
        .and_then(|resource| resource.media_url.clone())
        .or_else(|| media.media_url.clone());
    let availability = resource
        .and_then(|resource| resource.availability.clone())
        .or_else(|| media.availability.clone());
    let video = resource
        .and_then(|resource| resource.video.clone())
        .or_else(|| media.video.clone());

    if media_url.is_none() && availability.is_none() && video.is_none() {
        return None;
    }

    let video = match video.as_ref().map(convert_media_video).transpose() {
        Ok(video) => video,
        Err(error) => return Some(Err(error)),
    };

    Some(Ok(MediaResource {
        media_id,
        recorded_at: resource
            .and_then(|resource| resource.fetched_at)
            .or(media.fetched_at)
            .unwrap_or(now),
        media_url,
        availability,
        video,
    }))
}

pub(super) fn convert_media_geometry(geometry: &SubmitMediaGeometry) -> AppResult<MediaGeometry> {
    ensure_positive("media.geometry.width", geometry.width)?;
    ensure_positive("media.geometry.height", geometry.height)?;
    Ok(MediaGeometry {
        w: geometry.width,
        h: geometry.height,
        focus_rects: geometry
            .focus_rects
            .iter()
            .map(convert_media_rect)
            .collect::<AppResult<_>>()?,
    })
}

pub(super) fn convert_media_rect(
    rect: &SubmitMediaRect,
) -> AppResult<crate::tweet_model::MediaRect> {
    ensure_nonnegative("media.rect.x", rect.x.into())?;
    ensure_nonnegative("media.rect.y", rect.y.into())?;
    ensure_positive("media.rect.width", rect.width)?;
    ensure_positive("media.rect.height", rect.height)?;
    Ok(crate::tweet_model::MediaRect {
        x: rect.x,
        y: rect.y,
        w: rect.width,
        h: rect.height,
    })
}

pub(super) fn convert_media_variants(
    variants: &SubmitMediaVariants,
) -> AppResult<MediaSizeVariants> {
    Ok(MediaSizeVariants {
        large: variants
            .large
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        medium: variants
            .medium
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        small: variants
            .small
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        thumb: variants
            .thumb
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
    })
}

pub(super) fn convert_media_variant(variant: &SubmitMediaVariant) -> AppResult<MediaSizeVariant> {
    ensure_positive("media.variant.width", variant.width)?;
    ensure_positive("media.variant.height", variant.height)?;
    Ok(MediaSizeVariant {
        w: variant.width,
        h: variant.height,
        resize_mode: variant.resize_mode.clone(),
    })
}

pub(super) fn convert_media_tag(tag: &SubmitMediaTag) -> AppResult<MediaTag> {
    Ok(MediaTag {
        user_id: parse_optional_i64_id(tag.user_id.as_deref(), "media.taggedUsers.userId")?,
        kind: tag.kind.clone(),
    })
}

pub(super) fn convert_media_details(details: &SubmitMediaDetails) -> MediaDetails {
    MediaDetails {
        title: details.title.clone(),
        description: details.description.clone(),
        site_url: details.site_url.clone(),
        is_embeddable: details.is_embeddable,
        is_monetizable: details.is_monetizable,
    }
}

pub(super) fn convert_media_video(video: &SubmitMediaVideo) -> AppResult<MediaVideo> {
    if let Some([w, h]) = video.aspect_ratio {
        ensure_positive("media.video.aspectRatio[0]", w)?;
        ensure_positive("media.video.aspectRatio[1]", h)?;
    }
    let [aspect_ratio_w, aspect_ratio_h] = video.aspect_ratio.unwrap_or([0, 0]);
    Ok(MediaVideo {
        aspect_ratio_w: video.aspect_ratio.map(|_| aspect_ratio_w),
        aspect_ratio_h: video.aspect_ratio.map(|_| aspect_ratio_h),
        duration_ms: validate_optional_nonnegative("media.video.durationMs", video.duration_ms)?,
        variants: video
            .variants
            .iter()
            .map(convert_video_variant)
            .collect::<AppResult<_>>()?,
    })
}

pub(super) fn convert_video_variant(variant: &SubmitVideoVariant) -> AppResult<VideoVariant> {
    Ok(VideoVariant {
        content_type: variant.content_type.clone(),
        bitrate: validate_optional_nonnegative(
            "media.video.variant.bitrate",
            variant.bitrate.map(i64::from),
        )?
        .map(|value| {
            value
                .try_into()
                .map_err(|_| AppError::bad_request("media.video.variant.bitrate is too large"))
        })
        .transpose()?,
        url: variant.url.clone(),
    })
}

pub(super) async fn convert_annotated_text(
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

pub(super) fn convert_resolved_url(value: &SubmitResolvedUrl) -> ResolvedUrl {
    ResolvedUrl {
        url: value.url.clone(),
        expanded_url: value.expanded_url.clone(),
        display_text: value.display_text.clone(),
    }
}

pub(super) fn collect_text_refs(
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

pub(super) fn dedupe_refs(
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

pub(super) fn dedupe_media_refs(media_refs: &mut Vec<TweetMediaRef>) {
    let mut seen = HashSet::new();
    media_refs.retain(|reference| seen.insert(reference.media_id));
}
