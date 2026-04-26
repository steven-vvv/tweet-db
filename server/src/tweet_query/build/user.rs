use super::*;

pub(in crate::tweet_query) async fn build_user_json(
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

pub(in crate::tweet_query) async fn build_user_profile_json(
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

pub(in crate::tweet_query) fn build_user_stats_json(stats: &DbUserStats) -> Value {
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

pub(in crate::tweet_query) fn build_user_features_json(features: &DbUserFeatures) -> Value {
    json!({
        "canDm": features.can_dm,
        "canTagMedia": features.can_tag_media,
        "isProtected": features.is_protected,
        "canBeSubscribed": features.can_be_subscribed,
    })
}

pub(in crate::tweet_query) async fn build_user_identity_json(
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

pub(in crate::tweet_query) async fn build_user_verification_json(
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

pub(in crate::tweet_query) async fn build_user_disclosure_json(
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

pub(in crate::tweet_query) async fn build_user_professional_json(
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
