use super::*;

pub(in crate::tweet_submit) async fn convert_user_snapshot(
    store: &TweetStore<'_>,
    lookup_ids: Option<&SubmitLookupIds>,
    user_id: i64,
    user: &SubmitUser,
    profile: &SubmitUserProfile,
    now: OffsetDateTime,
) -> AppResult<UserSnapshot> {
    Ok(UserSnapshot {
        user_id,
        recorded_at: postgres_timestamptz(profile.fetched_at.or(user.fetched_at).unwrap_or(now)),
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

pub(in crate::tweet_submit) fn convert_user_stats(
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
        recorded_at: postgres_timestamptz(stats.fetched_at.or(parent_fetched_at).unwrap_or(now)),
        followers: stats.followers,
        following: stats.following,
        likes: stats.likes,
        media_posts: stats.media_posts,
        tweets: stats.tweets,
        listed: stats.listed,
    })
}

pub(in crate::tweet_submit) async fn convert_user_professional(
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

pub(in crate::tweet_submit) fn convert_user_identity(
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

pub(in crate::tweet_submit) fn convert_user_features(
    features: &SubmitUserFeatures,
) -> UserFeatures {
    UserFeatures {
        can_dm: features.can_dm,
        can_tag_media: features.can_tag_media,
        is_protected: features.is_protected,
        can_be_subscribed: features.can_be_subscribed,
    }
}
