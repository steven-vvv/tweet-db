use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    state::AppState,
    string_dict::{StringDictCache, StringDictValue, StringSemantic},
    tweet_model::{Hashtag, Symbol, TweetMediaRef, UserCategory},
    tweet_store::TweetStore,
};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryTweetRequest {
    #[serde(default)]
    pub users: Vec<QueryIdSelector>,
    #[serde(default)]
    pub tweets: Vec<QueryIdSelector>,
    #[serde(default)]
    pub media: Vec<QueryIdSelector>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryIdSelector {
    pub id: String,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryTweetResponse {
    pub summary: QuerySummary,
    pub users: Vec<QueryObjectResult>,
    pub tweets: Vec<QueryObjectResult>,
    pub media: Vec<QueryObjectResult>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuerySummary {
    pub total: usize,
    pub found: usize,
    pub missing: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryObjectResult {
    pub id: Option<String>,
    pub status: QueryObjectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryObjectStatus {
    Found,
    Missing,
    Failed,
}

pub async fn query_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<QueryTweetRequest>,
) -> AppResult<Json<QueryTweetResponse>> {
    let _session = auth::require_registered_session(session)?;
    let total = payload.users.len() + payload.tweets.len() + payload.media.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "query item count {total} exceeds ingest.max_items_per_batch"
        )));
    }

    let store = TweetStore::new(&state.db, &state.string_dict);
    let (users, tweets, media) = tokio::try_join!(
        query_users(&store, &state.string_dict, &payload.users),
        query_tweet_objects(&store, &state.string_dict, &payload.tweets),
        query_media_objects(&store, &state.string_dict, &payload.media),
    )?;

    let mut response = QueryTweetResponse {
        users,
        tweets,
        media,
        ..Default::default()
    };
    response.summary = summarize_results(&response);
    Ok(Json(response))
}

async fn query_users(
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

async fn query_tweet_objects(
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

async fn query_media_objects(
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

async fn build_user_json(
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

async fn build_user_profile_json(
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

fn build_user_stats_json(stats: &DbUserStats) -> Value {
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

fn build_user_features_json(features: &DbUserFeatures) -> Value {
    json!({
        "canDm": features.can_dm,
        "canTagMedia": features.can_tag_media,
        "isProtected": features.is_protected,
        "canBeSubscribed": features.can_be_subscribed,
    })
}

async fn build_user_identity_json(
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

async fn build_user_verification_json(
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

async fn build_user_disclosure_json(
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

async fn build_user_professional_json(
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

async fn build_tweet_json(
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

async fn build_tweet_place_json(
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

async fn build_tweet_content_json(
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

fn conversation_json(tweet: &DbTweet) -> Value {
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

fn build_tweet_stats_json(stats: &DbTweetStats) -> Value {
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

fn build_tweet_edit_json(edit: &DbTweetEdit) -> Value {
    json!({
        "versionIds": edit.version_ids.iter().map(i64::to_string).collect::<Vec<_>>(),
        "editableUntilAt": format_time_opt(edit.editable_until),
        "remainingEdits": edit.remaining_edits.map(|value| value.to_string()),
    })
}

async fn build_tweet_policy_json(
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

async fn build_tweet_community_note_json(
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

async fn build_media_json(
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

async fn build_media_variants_json(
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

async fn build_media_variant_json(
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

async fn build_media_tags_json(
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

async fn build_media_resource_json(
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

async fn build_media_video_json(
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

async fn build_annotated_text_json(
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

fn resolved_url_json(url: &DbResolvedUrl) -> Value {
    json!({
        "url": url.url,
        "expandedUrl": url.expanded_url,
        "displayText": url.display_text,
    })
}

fn geo_point_json(point: &DbGeoPoint) -> Value {
    json!({
        "longitude": point.longitude,
        "latitude": point.latitude,
    })
}

fn media_geometry_json(geometry: &DbMediaGeometry) -> Value {
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

fn media_details_json(details: &DbMediaDetails) -> Value {
    json!({
        "title": details.title,
        "description": details.description,
        "siteUrl": details.site_url,
        "isEmbeddable": details.is_embeddable,
        "isMonetizable": details.is_monetizable,
    })
}

fn media_entity_json(entity: &DbMediaEntity) -> Value {
    json!({
        "mediaId": entity.media_id.to_string(),
        "range": range_json(entity.range_start, entity.range_end),
        "displayText": empty_string_as_none(&entity.display_text),
        "expandedUrl": empty_string_as_none(&entity.expanded_url),
        "url": empty_string_as_none(&entity.url),
        "origin": media_origin_json(entity.origin_tweet_id, entity.origin_user_id),
    })
}

fn media_origin_json(origin_tweet_id: Option<i64>, origin_user_id: Option<i64>) -> Option<Value> {
    (origin_tweet_id.is_some() || origin_user_id.is_some()).then(|| {
        json!({
            "tweetId": origin_tweet_id.map(|id| id.to_string()),
            "userId": origin_user_id.map(|id| id.to_string()),
        })
    })
}

fn range_json(start: i32, end: i32) -> Value {
    json!({
        "start": start,
        "end": end,
    })
}

fn display_range_json(start: Option<i32>, end: Option<i32>) -> Option<Value> {
    match (start, end) {
        (Some(start), Some(end)) => Some(range_json(start, end)),
        _ => None,
    }
}

async fn resolve_optional_string(
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

async fn resolve_string(
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

async fn resolve_string_list(
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

fn ensure_semantic(
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

fn collect_optional_annotated_text_lookup_ids(
    text: Option<&DbAnnotatedText>,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    if let Some(text) = text {
        collect_annotated_text_lookup_ids(text, hashtag_ids, symbol_ids);
    }
}

fn collect_annotated_text_lookup_ids(
    text: &DbAnnotatedText,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    hashtag_ids.extend(text.hashtags.iter().map(|reference| reference.hashtag_id));
    symbol_ids.extend(text.symbols.iter().map(|reference| reference.symbol_id));
}

fn summarize_results(response: &QueryTweetResponse) -> QuerySummary {
    let mut summary = QuerySummary::default();
    for result in response
        .users
        .iter()
        .chain(response.tweets.iter())
        .chain(response.media.iter())
    {
        summary.total += 1;
        match result.status {
            QueryObjectStatus::Found => summary.found += 1,
            QueryObjectStatus::Missing => summary.missing += 1,
            QueryObjectStatus::Failed => summary.failed += 1,
        }
    }
    summary
}

fn found_result(id: Option<String>, data: Value) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Found,
        data: Some(data),
        error: None,
    }
}

fn missing_result(id: Option<String>) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Missing,
        data: None,
        error: None,
    }
}

fn failed_result(id: Option<String>, error: impl Into<String>) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Failed,
        data: None,
        error: Some(error.into()),
    }
}

fn selection_failed_or_missing_result(selection: QuerySelectionI64) -> QueryObjectResult {
    if let Some(error) = selection.error {
        failed_result(Some(selection.id), error)
    } else {
        missing_result(Some(selection.id))
    }
}

fn parse_i64_selections(selectors: &[QueryIdSelector], field: &str) -> Vec<QuerySelectionI64> {
    selectors
        .iter()
        .map(|selector| match parse_i64_id(&selector.id, field) {
            Ok(parsed) => QuerySelectionI64 {
                id: selector.id.clone(),
                parsed: Some(parsed),
                error: None,
            },
            Err(error) => QuerySelectionI64 {
                id: selector.id.clone(),
                parsed: None,
                error: Some(error.to_string()),
            },
        })
        .collect()
}

fn collect_unique_valid_ids(selections: &[QuerySelectionI64]) -> Vec<i64> {
    let mut seen = HashSet::new();
    selections
        .iter()
        .filter_map(|selection| selection.parsed)
        .filter(|id| seen.insert(*id))
        .collect()
}

fn decode_i64_map<T: DeserializeOwned>(
    values: HashMap<i64, Value>,
    label: &str,
) -> HashMap<i64, Result<T, String>> {
    values
        .into_iter()
        .map(|(id, value)| {
            let decoded = serde_json::from_value(value)
                .map_err(|error| format!("failed to decode {label} {id}: {error}"));
            (id, decoded)
        })
        .collect()
}

fn decode_string_map<T: DeserializeOwned>(
    values: HashMap<String, Value>,
    label: &str,
) -> HashMap<String, Result<T, String>> {
    values
        .into_iter()
        .map(|(id, value)| {
            let decoded = serde_json::from_value(value)
                .map_err(|error| format!("failed to decode {label} {id}: {error}"));
            (id, decoded)
        })
        .collect()
}

fn decode_required<'a, K, T>(
    values: &'a HashMap<K, Result<T, String>>,
    key: &K,
) -> Option<Result<&'a T, String>>
where
    K: Eq + Hash,
{
    match values.get(key) {
        Some(Ok(value)) => Some(Ok(value)),
        Some(Err(error)) => Some(Err(error.clone())),
        None => None,
    }
}

fn decode_optional<'a, K, T>(
    values: &'a HashMap<K, Result<T, String>>,
    key: &K,
) -> Result<Option<&'a T>, String>
where
    K: Eq + Hash,
{
    match values.get(key) {
        Some(Ok(value)) => Ok(Some(value)),
        Some(Err(error)) => Err(error.clone()),
        None => Ok(None),
    }
}

fn parse_i64_id(value: &str, field: &str) -> AppResult<i64> {
    value
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 64-bit integer")))
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

fn format_time_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_time)
}

fn empty_string_as_none(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[derive(Debug)]
struct QuerySelectionI64 {
    id: String,
    parsed: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DbTwitterUser {
    id: i64,
    #[serde(default, with = "time::serde::rfc3339::option")]
    registered_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
struct DbResolvedUrl {
    url: String,
    expanded_url: String,
    display_text: String,
}

#[derive(Debug, Deserialize)]
struct DbHashtagRef {
    hashtag_id: i32,
    range_start: i32,
    range_end: i32,
}

#[derive(Debug, Deserialize)]
struct DbSymbolRef {
    symbol_id: i32,
    range_start: i32,
    range_end: i32,
}

#[derive(Debug, Deserialize)]
struct DbUrlEntity {
    url: String,
    expanded_url: String,
    display_text: String,
    range_start: i32,
    range_end: i32,
}

#[derive(Debug, Deserialize)]
struct DbMentionEntity {
    user_id: i64,
    range_start: i32,
    range_end: i32,
}

#[derive(Debug, Deserialize)]
struct DbMediaEntity {
    media_id: i64,
    range_start: i32,
    range_end: i32,
    display_text: String,
    expanded_url: String,
    url: String,
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DbTextStyleRange {
    range_start: i32,
    range_end: i32,
    #[serde(default)]
    style_ids: Vec<i16>,
}

#[derive(Debug, Deserialize)]
struct DbAnnotatedText {
    body: String,
    display_range_start: Option<i32>,
    display_range_end: Option<i32>,
    #[serde(default)]
    hashtags: Vec<DbHashtagRef>,
    #[serde(default)]
    symbols: Vec<DbSymbolRef>,
    #[serde(default)]
    urls: Vec<DbUrlEntity>,
    #[serde(default)]
    mentions: Vec<DbMentionEntity>,
    #[serde(default)]
    media_refs: Vec<DbMediaEntity>,
    #[serde(default)]
    styles: Vec<DbTextStyleRange>,
}

#[derive(Debug, Deserialize)]
struct DbUserVerification {
    is_blue_verified: Option<bool>,
    verified_type_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
struct DbUserDisclosure {
    relation_id: Option<i16>,
    subject_id: Option<i64>,
    subject_handle: Option<String>,
    subject_name: Option<String>,
    subject_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DbUserIdentity {
    verification: Option<DbUserVerification>,
    disclosure: Option<DbUserDisclosure>,
    parody_label_id: Option<i16>,
    has_completed_new_account_review: Option<bool>,
    is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DbUserFeatures {
    can_dm: Option<bool>,
    can_tag_media: Option<bool>,
    is_protected: Option<bool>,
    can_be_subscribed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DbUserProfessional {
    professional_id: Option<i64>,
    professional_type_id: Option<i16>,
    #[serde(default)]
    category_ids: Vec<i16>,
}

#[derive(Debug, Deserialize)]
struct DbUserSnapshot {
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    display_name: String,
    user_name: String,
    avatar_url: Option<String>,
    uses_default_avatar: Option<bool>,
    avatar_shape_id: Option<i16>,
    banner_url: Option<String>,
    location: Option<String>,
    bio: Option<DbAnnotatedText>,
    #[serde(default)]
    profile_links: Vec<DbResolvedUrl>,
    identity: Option<DbUserIdentity>,
    features: Option<DbUserFeatures>,
    professional: Option<DbUserProfessional>,
    #[serde(default)]
    pinned_tweet_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct DbUserStats {
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    followers: Option<i64>,
    following: Option<i64>,
    likes: Option<i64>,
    media_posts: Option<i64>,
    tweets: Option<i64>,
    listed: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DbGeoPoint {
    longitude: f64,
    latitude: f64,
}

#[derive(Debug, Deserialize)]
struct DbTweetPlace {
    id: String,
    name: Option<String>,
    full_name: Option<String>,
    country_id: Option<i16>,
    country_code_id: Option<i16>,
    kind_id: Option<i16>,
    boundary: Option<Vec<DbGeoPoint>>,
}

#[derive(Debug, Deserialize)]
struct DbTweet {
    id: i64,
    #[serde(with = "time::serde::rfc3339")]
    published_at: OffsetDateTime,
    source_id: Option<i16>,
    author_id: i64,
    place_id: Option<String>,
    legacy_text: DbAnnotatedText,
    note_id: Option<String>,
    note_text: Option<DbAnnotatedText>,
    language_id: Option<i16>,
    conversation_id: i64,
    reply_to_tweet_id: Option<i64>,
    reply_to_user_id: Option<i64>,
    quote_tweet_id: Option<i64>,
    quote_permalink: Option<DbResolvedUrl>,
    repost_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DbTweetEdit {
    #[serde(default)]
    version_ids: Vec<i64>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    editable_until: Option<OffsetDateTime>,
    remaining_edits: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct DbTweetPolicy {
    reply_policy_id: Option<i16>,
    followers_only: Option<bool>,
    is_possibly_sensitive: Option<bool>,
    #[serde(default)]
    available_action_ids: Vec<i16>,
    is_media_visibility_restricted: Option<bool>,
    paid_promotion: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DbTweetStats {
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    views: Option<i64>,
    replies: Option<i64>,
    reposts: Option<i64>,
    quotes: Option<i64>,
    likes: Option<i64>,
    bookmarks: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DbTweetCommunityNote {
    note_id: Option<i64>,
    title: Option<String>,
    short_title: Option<String>,
    subtitle: Option<DbAnnotatedText>,
    footer: Option<DbAnnotatedText>,
    destination_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DbMediaRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Deserialize)]
struct DbMediaGeometry {
    w: i32,
    h: i32,
    #[serde(default)]
    focus_rects: Vec<DbMediaRect>,
}

#[derive(Debug, Deserialize)]
struct DbMediaSizeVariant {
    w: i32,
    h: i32,
    resize_mode_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
struct DbMediaSizeVariants {
    large: Option<DbMediaSizeVariant>,
    medium: Option<DbMediaSizeVariant>,
    small: Option<DbMediaSizeVariant>,
    thumb: Option<DbMediaSizeVariant>,
}

#[derive(Debug, Deserialize)]
struct DbMediaTag {
    user_id: Option<i64>,
    kind_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
struct DbMediaDetails {
    title: Option<String>,
    description: Option<String>,
    site_url: Option<String>,
    is_embeddable: Option<bool>,
    is_monetizable: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DbVideoVariant {
    content_type_id: Option<i16>,
    bitrate: Option<i32>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct DbMediaVideo {
    aspect_ratio_w: Option<i32>,
    aspect_ratio_h: Option<i32>,
    duration_ms: Option<i64>,
    #[serde(default)]
    variants: Vec<DbVideoVariant>,
}

#[derive(Debug, Deserialize)]
struct DbMedia {
    id: i64,
    #[serde(rename = "type")]
    media_type: String,
    alt_text: Option<String>,
    grok_post_id: Option<uuid::Uuid>,
    geometry: Option<DbMediaGeometry>,
    size_variants: Option<DbMediaSizeVariants>,
    #[serde(default)]
    tagged_users: Vec<DbMediaTag>,
    #[serde(default)]
    sensitivity_warning_ids: Vec<i16>,
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
    details: Option<DbMediaDetails>,
}

#[derive(Debug, Deserialize)]
struct DbMediaResource {
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    media_url: Option<String>,
    availability_id: Option<i16>,
    video: Option<DbMediaVideo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_request_accepts_three_root_lists() {
        let request: QueryTweetRequest = serde_json::from_value(json!({
            "users": [],
            "tweets": [],
            "media": []
        }))
        .unwrap();

        assert_eq!(request.users.len(), 0);
        assert_eq!(request.tweets.len(), 0);
        assert_eq!(request.media.len(), 0);
    }

    #[test]
    fn summary_counts_found_missing_and_failed_results() {
        let response = QueryTweetResponse {
            users: vec![found_result(Some("1".to_owned()), json!({"id": "1"}))],
            tweets: vec![missing_result(Some("2".to_owned()))],
            media: vec![failed_result(Some("3".to_owned()), "broken")],
            ..Default::default()
        };

        let summary = summarize_results(&response);

        assert_eq!(summary.total, 3);
        assert_eq!(summary.found, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(summary.failed, 1);
    }
}
