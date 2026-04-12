use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    error::{AppError, AppResult},
    state::AppState,
    tweet_model::{
        AnnotatedText, GeoPoint, Hashtag, HashtagRef, Media, MediaDetails, MediaEntity,
        MediaGeometry, MediaResource, MediaSizeVariant, MediaSizeVariants, MediaTag, MediaType,
        MediaVideo, MentionEntity, ResolvedUrl, Symbol, SymbolRef, TextStyleRange, Tweet,
        TweetCommunityNote, TweetEdit, TweetHashtagRef, TweetMediaRef, TweetMentionRef, TweetPlace,
        TweetPolicy, TweetStats, TweetSymbolRef, TwitterUser, UrlEntity, UserCategory,
        UserDisclosure, UserFeatures, UserIdentity, UserProfessional, UserSnapshot, UserStats,
        UserVerification, VideoVariant,
    },
    tweet_store::{ConditionalWrite, TweetStore},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetRequest {
    #[serde(default)]
    pub users: Vec<SubmitUser>,
    #[serde(default)]
    pub tweets: Vec<SubmitTweet>,
    #[serde(default)]
    pub media: Vec<SubmitMedia>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUser {
    pub id: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    #[serde(
        rename = "registeredAt",
        alias = "createdAt",
        default,
        with = "time::serde::rfc3339::option"
    )]
    pub registered_at: Option<OffsetDateTime>,
    pub profile: Option<SubmitUserProfile>,
    #[serde(default)]
    pub pinned_tweet_ids: Vec<String>,
    pub identity: Option<SubmitUserIdentity>,
    pub professional: Option<SubmitUserProfessional>,
    pub stats: Option<SubmitUserStats>,
    pub features: Option<SubmitUserFeatures>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserProfile {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub display_name: String,
    pub user_name: String,
    pub avatar_url: Option<String>,
    pub uses_default_avatar: Option<bool>,
    pub avatar_shape: Option<String>,
    pub banner_url: Option<String>,
    pub location: Option<String>,
    pub bio: Option<SubmitAnnotatedText>,
    #[serde(default)]
    pub profile_links: Vec<SubmitResolvedUrl>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserStats {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub followers: Option<i64>,
    pub following: Option<i64>,
    pub likes: Option<i64>,
    pub media_posts: Option<i64>,
    pub tweets: Option<i64>,
    pub listed: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserFeatures {
    pub can_dm: Option<bool>,
    pub can_tag_media: Option<bool>,
    pub is_protected: Option<bool>,
    pub can_be_subscribed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserIdentity {
    pub verification: Option<SubmitUserVerification>,
    pub disclosure: Option<SubmitUserDisclosure>,
    pub parody_label: Option<String>,
    pub has_completed_new_account_review: Option<bool>,
    pub is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserVerification {
    pub is_blue_verified: Option<bool>,
    #[serde(rename = "type")]
    pub verification_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserDisclosure {
    pub relation: Option<String>,
    pub subject_id: Option<String>,
    pub subject_handle: Option<String>,
    pub subject_name: Option<String>,
    pub subject_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserProfessional {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub professional_type: Option<String>,
    #[serde(default)]
    pub categories: Vec<SubmitUserCategory>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUserCategory {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweet {
    pub id: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    #[serde(
        rename = "publishedAt",
        alias = "createdAt",
        with = "time::serde::rfc3339"
    )]
    pub published_at: OffsetDateTime,
    pub source: Option<String>,
    pub author_id: String,
    pub place: Option<SubmitTweetPlace>,
    pub content: SubmitTweetContent,
    pub conversation: SubmitTweetConversation,
    pub stats: Option<SubmitTweetStats>,
    pub edit: Option<SubmitTweetEdit>,
    pub policy: Option<SubmitTweetPolicy>,
    pub community_note: Option<SubmitTweetCommunityNote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetContent {
    pub legacy_text: SubmitAnnotatedText,
    pub note: Option<SubmitTweetNote>,
    #[serde(default)]
    pub media_ids: Vec<String>,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetNote {
    pub id: Option<String>,
    pub text: SubmitAnnotatedText,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetConversation {
    pub conversation_id: String,
    pub reply_to: Option<SubmitTweetReplyTarget>,
    pub quote: Option<SubmitTweetQuote>,
    pub repost_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetReplyTarget {
    pub tweet_id: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetQuote {
    pub tweet_id: String,
    pub permalink: Option<SubmitResolvedUrl>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetStats {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub views: Option<String>,
    pub replies: Option<i64>,
    pub reposts: Option<i64>,
    pub quotes: Option<i64>,
    pub likes: Option<i64>,
    pub bookmarks: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetEdit {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub version_ids: Vec<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub editable_until_at: Option<OffsetDateTime>,
    pub remaining_edits: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetPolicy {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub reply_policy: Option<String>,
    pub followers_only: Option<bool>,
    pub is_possibly_sensitive: Option<bool>,
    #[serde(default)]
    pub available_actions: Vec<String>,
    pub is_media_visibility_restricted: Option<bool>,
    pub paid_promotion: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetCommunityNote {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub id: Option<String>,
    pub title: Option<String>,
    pub short_title: Option<String>,
    pub subtitle: Option<SubmitAnnotatedText>,
    pub footer: Option<SubmitAnnotatedText>,
    pub destination_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetPlace {
    pub id: Option<String>,
    pub name: Option<String>,
    pub full_name: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub kind: Option<String>,
    pub boundary: Option<Vec<SubmitGeoPoint>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMedia {
    pub id: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    #[serde(rename = "type")]
    pub media_type: SubmitMediaType,
    pub media_url: Option<String>,
    pub alt_text: Option<String>,
    pub grok_post_id: Option<String>,
    pub geometry: Option<SubmitMediaGeometry>,
    pub variants: Option<SubmitMediaVariants>,
    #[serde(default)]
    pub tagged_users: Vec<SubmitMediaTag>,
    #[serde(default)]
    pub sensitivity_warnings: Vec<String>,
    pub availability: Option<String>,
    pub video: Option<SubmitMediaVideo>,
    pub origin: Option<SubmitMediaOrigin>,
    pub details: Option<SubmitMediaDetails>,
    pub resource: Option<SubmitMediaResource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmitMediaType {
    Photo,
    Video,
    AnimatedGif,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaResource {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    pub media_url: Option<String>,
    pub availability: Option<String>,
    pub video: Option<SubmitMediaVideo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaOrigin {
    pub tweet_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaDetails {
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_url: Option<String>,
    pub is_embeddable: Option<bool>,
    pub is_monetizable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaGeometry {
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub focus_rects: Vec<SubmitMediaRect>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaVariants {
    pub large: Option<SubmitMediaVariant>,
    pub medium: Option<SubmitMediaVariant>,
    pub small: Option<SubmitMediaVariant>,
    pub thumb: Option<SubmitMediaVariant>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaVariant {
    pub width: i32,
    pub height: i32,
    pub resize_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaTag {
    pub user_id: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaVideo {
    pub aspect_ratio: Option<[i32; 2]>,
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub variants: Vec<SubmitVideoVariant>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubmitVideoVariant {
    pub content_type: String,
    pub bitrate: Option<i32>,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAnnotatedText {
    pub text: String,
    pub display_range: Option<SubmitTextRange>,
    #[serde(default)]
    pub entities: SubmitTextEntities,
    #[serde(default)]
    pub styles: Vec<SubmitTextStyleRange>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTextEntities {
    #[serde(default)]
    pub hashtags: Vec<SubmitHashtagEntity>,
    #[serde(default)]
    pub symbols: Vec<SubmitSymbolEntity>,
    #[serde(default)]
    pub urls: Vec<SubmitUrlEntity>,
    #[serde(default)]
    pub mentions: Vec<SubmitMentionEntity>,
    #[serde(default)]
    pub media: Vec<SubmitMediaEntity>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTextRange {
    pub start: i32,
    pub end: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTextStyleRange {
    pub range: SubmitTextRange,
    #[serde(default)]
    pub styles: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitResolvedUrl {
    pub url: String,
    pub expanded_url: String,
    pub display_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitHashtagEntity {
    pub text: String,
    pub range: SubmitTextRange,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitSymbolEntity {
    pub text: String,
    pub range: Option<SubmitTextRange>,
    pub ticker: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitUrlEntity {
    pub url: String,
    pub expanded_url: String,
    pub display_text: String,
    pub range: SubmitTextRange,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMentionEntity {
    pub user_id: String,
    pub name: Option<String>,
    pub user_name: Option<String>,
    pub range: SubmitTextRange,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMediaEntity {
    pub media_id: String,
    pub range: Option<SubmitTextRange>,
    pub display_text: Option<String>,
    pub expanded_url: Option<String>,
    pub url: Option<String>,
    pub origin: Option<SubmitMediaOrigin>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubmitGeoPoint {
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTweetResponse {
    pub summary: SubmitSummary,
    pub users: Vec<SubmitObjectResult>,
    pub tweets: Vec<SubmitObjectResult>,
    pub media: Vec<SubmitObjectResult>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubmitSummary {
    pub total: usize,
    pub accepted: usize,
    pub skipped: usize,
    pub partial: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitObjectResult {
    pub id: Option<String>,
    pub status: SubmitObjectStatus,
    pub operations: Vec<SubmitOperationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubmitObjectStatus {
    Accepted,
    Skipped,
    Partial,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitOperationResult {
    pub name: &'static str,
    pub status: SubmitOperationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubmitOperationStatus {
    Accepted,
    Skipped,
    Failed,
}

struct ObjectResultBuilder {
    id: Option<String>,
    operations: Vec<SubmitOperationResult>,
    fatal_error: Option<String>,
}

impl ObjectResultBuilder {
    fn new(id: Option<String>) -> Self {
        Self {
            id,
            operations: Vec::new(),
            fatal_error: None,
        }
    }

    fn accepted(&mut self, name: &'static str, reason: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Accepted, Some(reason.into()));
    }

    fn skipped(&mut self, name: &'static str, reason: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Skipped, Some(reason.into()));
    }

    fn failed(&mut self, name: &'static str, error: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Failed, Some(error.into()));
    }

    fn fatal(&mut self, error: impl Into<String>) {
        self.fatal_error = Some(error.into());
    }

    fn operation(
        &mut self,
        name: &'static str,
        status: SubmitOperationStatus,
        reason: Option<String>,
    ) {
        self.operations.push(SubmitOperationResult {
            name,
            status,
            reason,
        });
    }

    fn finish(self) -> SubmitObjectResult {
        let has_accepted = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Accepted);
        let has_skipped = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Skipped);
        let has_failed = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Failed);

        let status = if has_failed && (has_accepted || has_skipped) {
            SubmitObjectStatus::Partial
        } else if has_failed || self.fatal_error.is_some() {
            SubmitObjectStatus::Failed
        } else if has_accepted {
            SubmitObjectStatus::Accepted
        } else {
            SubmitObjectStatus::Skipped
        };

        SubmitObjectResult {
            id: self.id,
            status,
            operations: self.operations,
            error: self.fatal_error,
        }
    }
}

pub async fn submit_tweets(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<SubmitTweetRequest>,
) -> AppResult<Json<SubmitTweetResponse>> {
    let _session = auth::require_admin_session(session)?;
    let total = payload.users.len() + payload.tweets.len() + payload.media.len();
    if total > state.settings.config.ingest.max_items_per_batch {
        return Err(AppError::bad_request(format!(
            "submit item count {total} exceeds ingest.max_items_per_batch"
        )));
    }

    let store = TweetStore::new(&state.db, &state.string_dict);
    let stats_interval = state.settings.config.ingest.stats_sample_interval_seconds;
    let mut prepared = prepare_submit_batch(&store, payload).await;
    execute_prepared_submit(&store, &mut prepared, stats_interval).await;

    let mut response = SubmitTweetResponse {
        users: prepared
            .user_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        tweets: prepared
            .tweet_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        media: prepared
            .media_results
            .into_iter()
            .map(ObjectResultBuilder::finish)
            .collect(),
        ..Default::default()
    };
    response.summary = summarize_results(&response);
    Ok(Json(response))
}

struct PreparedSubmitBatch {
    user_results: Vec<ObjectResultBuilder>,
    tweet_results: Vec<ObjectResultBuilder>,
    media_results: Vec<ObjectResultBuilder>,
    users: Vec<Indexed<TwitterUser>>,
    tweet_authors: Vec<Indexed<TwitterUser>>,
    user_snapshots: Vec<Indexed<UserSnapshot>>,
    user_stats: Vec<Indexed<UserStats>>,
    media: Vec<Indexed<Media>>,
    media_resources: Vec<Indexed<MediaResource>>,
    tweet_places: Vec<Indexed<TweetPlace>>,
    tweets: Vec<Indexed<Tweet>>,
    tweet_edits: Vec<Indexed<TweetEdit>>,
    tweet_policies: Vec<Indexed<TweetPolicy>>,
    tweet_community_notes: Vec<Indexed<TweetCommunityNote>>,
    tweet_stats: Vec<Indexed<TweetStats>>,
    tweet_relations: Vec<IndexedTweetRelations>,
}

impl PreparedSubmitBatch {
    fn new(user_count: usize, tweet_count: usize, media_count: usize) -> Self {
        Self {
            user_results: Vec::with_capacity(user_count),
            tweet_results: Vec::with_capacity(tweet_count),
            media_results: Vec::with_capacity(media_count),
            users: Vec::new(),
            tweet_authors: Vec::new(),
            user_snapshots: Vec::new(),
            user_stats: Vec::new(),
            media: Vec::new(),
            media_resources: Vec::new(),
            tweet_places: Vec::new(),
            tweets: Vec::new(),
            tweet_edits: Vec::new(),
            tweet_policies: Vec::new(),
            tweet_community_notes: Vec::new(),
            tweet_stats: Vec::new(),
            tweet_relations: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct Indexed<T> {
    index: usize,
    value: T,
}

struct IndexedTweetRelations {
    index: usize,
    tweet_id: i64,
    media_refs: Vec<TweetMediaRef>,
    mention_refs: Vec<TweetMentionRef>,
    hashtag_refs: Vec<TweetHashtagRef>,
    symbol_refs: Vec<TweetSymbolRef>,
}

struct SubmitLookupIds {
    user_categories: HashMap<i32, i16>,
    hashtags: HashMap<String, i32>,
    symbols: HashMap<String, i32>,
}

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

async fn prepare_submit_batch(
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

async fn execute_prepared_submit(
    store: &TweetStore<'_>,
    prepared: &mut PreparedSubmitBatch,
    stats_interval: i64,
) {
    let snapshots = prepared
        .user_snapshots
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let places = prepared
        .tweet_places
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let tweets = prepared
        .tweets
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let policies = prepared
        .tweet_policies
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let notes = prepared
        .tweet_community_notes
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let media = prepared
        .media
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    let resources = prepared
        .media_resources
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    if let Err(error) = store
        .preload_submit_batch_dicts(
            &snapshots, &places, &tweets, &policies, &notes, &media, &resources,
        )
        .await
    {
        let mut user_indices = HashSet::new();
        user_indices.extend(prepared.users.iter().map(|item| item.index));
        user_indices.extend(prepared.user_snapshots.iter().map(|item| item.index));
        user_indices.extend(prepared.user_stats.iter().map(|item| item.index));
        for index in user_indices {
            prepared.user_results[index].failed("dict_preload", error.to_string());
        }

        let mut tweet_indices = HashSet::new();
        tweet_indices.extend(prepared.tweet_authors.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_places.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweets.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_edits.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_policies.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_community_notes.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_stats.iter().map(|item| item.index));
        tweet_indices.extend(prepared.tweet_relations.iter().map(|item| item.index));
        for index in tweet_indices {
            prepared.tweet_results[index].failed("dict_preload", error.to_string());
        }

        let mut media_indices = HashSet::new();
        media_indices.extend(prepared.media.iter().map(|item| item.index));
        media_indices.extend(prepared.media_resources.iter().map(|item| item.index));
        for index in media_indices {
            prepared.media_results[index].failed("dict_preload", error.to_string());
        }
        return;
    }

    write_combined_user_batch(
        store,
        &prepared.users,
        &prepared.tweet_authors,
        &mut prepared.user_results,
        &mut prepared.tweet_results,
    )
    .await;
    write_user_snapshots_batch(store, &prepared.user_snapshots, &mut prepared.user_results).await;
    write_user_stats_batch(
        store,
        &prepared.user_stats,
        &mut prepared.user_results,
        stats_interval,
    )
    .await;
    write_media_batch(store, &prepared.media, &mut prepared.media_results).await;
    write_media_resources_batch(
        store,
        &prepared.media_resources,
        &mut prepared.media_results,
    )
    .await;
    write_tweet_places_batch(store, &prepared.tweet_places, &mut prepared.tweet_results).await;
    write_tweets_batch(store, &prepared.tweets, &mut prepared.tweet_results).await;
    write_tweet_edits_batch(store, &prepared.tweet_edits, &mut prepared.tweet_results).await;
    write_tweet_policies_batch(store, &prepared.tweet_policies, &mut prepared.tweet_results).await;
    write_tweet_community_notes_batch(
        store,
        &prepared.tweet_community_notes,
        &mut prepared.tweet_results,
    )
    .await;
    write_tweet_stats_batch(
        store,
        &prepared.tweet_stats,
        &mut prepared.tweet_results,
        stats_interval,
    )
    .await;
    replace_prepared_tweet_relations(store, prepared).await;
}

async fn write_combined_user_batch(
    store: &TweetStore<'_>,
    users: &[Indexed<TwitterUser>],
    tweet_authors: &[Indexed<TwitterUser>],
    user_results: &mut [ObjectResultBuilder],
    tweet_results: &mut [ObjectResultBuilder],
) {
    if users.is_empty() && tweet_authors.is_empty() {
        return;
    }

    #[derive(Clone)]
    struct CombinedUserWrite {
        value: TwitterUser,
        user_indices: Vec<usize>,
        tweet_indices: Vec<usize>,
    }

    let mut combined = HashMap::<i64, CombinedUserWrite>::new();
    for item in users {
        combined
            .entry(item.value.id)
            .and_modify(|entry| {
                if entry.value.registered_at.is_none() && item.value.registered_at.is_some() {
                    entry.value.registered_at = item.value.registered_at;
                }
                entry.user_indices.push(item.index);
            })
            .or_insert_with(|| CombinedUserWrite {
                value: item.value.clone(),
                user_indices: vec![item.index],
                tweet_indices: Vec::new(),
            });
    }
    for item in tweet_authors {
        combined
            .entry(item.value.id)
            .and_modify(|entry| entry.tweet_indices.push(item.index))
            .or_insert_with(|| CombinedUserWrite {
                value: item.value.clone(),
                user_indices: Vec::new(),
                tweet_indices: vec![item.index],
            });
    }

    let combined = combined.into_values().collect::<Vec<_>>();
    let values = combined
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.insert_users_changed(&values).await {
        Ok(changed) => {
            for item in &combined {
                if changed.contains(&item.value.id) {
                    for index in &item.user_indices {
                        user_results[*index].accepted("twitter_user", "inserted_or_filled");
                    }
                    for index in &item.tweet_indices {
                        tweet_results[*index].accepted("tweet_author", "inserted_minimal");
                    }
                } else {
                    for index in &item.user_indices {
                        user_results[*index].skipped("twitter_user", "unchanged_or_existing");
                    }
                    for index in &item.tweet_indices {
                        tweet_results[*index].skipped("tweet_author", "existing");
                    }
                }
            }
        }
        Err(error) => {
            for item in &combined {
                for index in &item.user_indices {
                    user_results[*index].failed("twitter_user", error.to_string());
                }
                for index in &item.tweet_indices {
                    tweet_results[*index].failed("tweet_author", error.to_string());
                }
            }
        }
    }
}

async fn write_user_snapshots_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<UserSnapshot>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.append_user_snapshots_if_changed_many(&values).await {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.user_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "user_snapshot", write)
                    }
                    None => results[item.index]
                        .failed("user_snapshot", "missing batch write status".to_owned()),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("user_snapshot", error.to_string());
            }
        }
    }
}

async fn write_user_stats_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<UserStats>],
    results: &mut [ObjectResultBuilder],
    stats_interval: i64,
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store
        .append_user_stats_if_changed_many(&values, stats_interval)
        .await
    {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.user_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "user_stats", write)
                    }
                    None => results[item.index].failed("user_stats", "missing batch write status"),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("user_stats", error.to_string());
            }
        }
    }
}

async fn write_media_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<Media>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_media_changed(&values).await {
        Ok(changed) => {
            for item in items {
                if changed.contains(&item.value.id) {
                    results[item.index].accepted("media", "inserted_or_filled");
                } else {
                    results[item.index].skipped("media", "unchanged_or_existing");
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media", error.to_string());
            }
        }
    }
}

async fn write_media_resources_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<MediaResource>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.append_media_resources_if_changed_many(&values).await {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.media_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "media_resource", write)
                    }
                    None => {
                        results[item.index].failed("media_resource", "missing batch write status")
                    }
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media_resource", error.to_string());
            }
        }
    }
}

async fn write_tweet_places_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetPlace>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let mut deduped = HashMap::<String, (TweetPlace, Vec<usize>)>::new();
    for item in items {
        deduped
            .entry(item.value.id.clone())
            .and_modify(|entry| entry.1.push(item.index))
            .or_insert_with(|| (item.value.clone(), vec![item.index]));
    }
    let values = deduped
        .values()
        .map(|(value, _)| value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_places_changed(&values).await {
        Ok(changed) => {
            for (place_id, (_, indices)) in deduped {
                if changed.contains(&place_id) {
                    for index in indices {
                        results[index].accepted("tweet_place", "inserted_or_filled");
                    }
                } else {
                    for index in indices {
                        results[index].skipped("tweet_place", "unchanged_or_existing");
                    }
                }
            }
        }
        Err(error) => {
            for (_, (_, indices)) in deduped {
                for index in indices {
                    results[index].failed("tweet_place", error.to_string());
                }
            }
        }
    }
}

async fn write_tweets_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<Tweet>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.insert_tweets_changed(&values).await {
        Ok(changed) => {
            for item in items {
                if changed.contains(&item.value.id) {
                    results[item.index].accepted("tweet", "inserted_or_filled");
                } else {
                    results[item.index].skipped("tweet", "unchanged_or_existing");
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet", error.to_string());
            }
        }
    }
}

async fn write_tweet_edits_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetEdit>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_edits_write_statuses(&values).await {
        Ok(statuses) => {
            record_tweet_write_statuses(items, results, &statuses, "tweet_edit", |value| {
                value.tweet_id
            })
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_edit", error.to_string());
            }
        }
    }
}

async fn write_tweet_policies_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetPolicy>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_tweet_policies_write_statuses(&values).await {
        Ok(statuses) => {
            record_tweet_write_statuses(items, results, &statuses, "tweet_policy", |value| {
                value.tweet_id
            })
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_policy", error.to_string());
            }
        }
    }
}

async fn write_tweet_community_notes_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetCommunityNote>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store
        .upsert_tweet_community_notes_write_statuses(&values)
        .await
    {
        Ok(statuses) => record_tweet_write_statuses(
            items,
            results,
            &statuses,
            "tweet_community_note",
            |value| value.tweet_id,
        ),
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_community_note", error.to_string());
            }
        }
    }
}

async fn write_tweet_stats_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<TweetStats>],
    results: &mut [ObjectResultBuilder],
    stats_interval: i64,
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store
        .append_tweet_stats_if_changed_many(&values, stats_interval)
        .await
    {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.tweet_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "tweet_stats", write)
                    }
                    None => results[item.index].failed("tweet_stats", "missing batch write status"),
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("tweet_stats", error.to_string());
            }
        }
    }
}

fn record_tweet_write_statuses<T>(
    items: &[Indexed<T>],
    results: &mut [ObjectResultBuilder],
    statuses: &HashMap<i64, ConditionalWrite>,
    operation: &'static str,
    key: impl Fn(&T) -> i64,
) {
    for item in items {
        match statuses.get(&key(&item.value)).copied() {
            Some(write) => record_conditional_write(&mut results[item.index], operation, write),
            None => results[item.index].failed(operation, "missing batch write status"),
        }
    }
}

fn record_relation_sync(
    result: &mut ObjectResultBuilder,
    operation: &'static str,
    status: Option<crate::tweet_store::RelationSyncStatus>,
) {
    use crate::tweet_store::RelationSyncStatus;

    match status {
        Some(RelationSyncStatus::Replaced) => result.accepted(operation, "replaced"),
        Some(RelationSyncStatus::ReplacedFiltered) => {
            result.accepted(operation, "replaced_with_missing_media_skipped")
        }
        Some(RelationSyncStatus::SkippedUnchanged) => result.skipped(operation, "unchanged"),
        Some(RelationSyncStatus::SkippedUnchangedFiltered) => {
            result.skipped(operation, "unchanged_with_missing_media_skipped")
        }
        Some(RelationSyncStatus::SkippedMissingTweet) => result.skipped(operation, "missing_tweet"),
        None => result.failed(operation, "missing relation sync status"),
    }
}

async fn replace_prepared_tweet_relations(
    store: &TweetStore<'_>,
    prepared: &mut PreparedSubmitBatch,
) {
    if prepared.tweet_relations.is_empty() {
        return;
    }

    let tweet_ids = prepared
        .tweet_relations
        .iter()
        .map(|item| item.tweet_id)
        .collect::<Vec<_>>();
    let media_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.media_refs.iter().cloned())
        .collect::<Vec<_>>();
    match store.sync_tweet_media_refs(&tweet_ids, &media_refs).await {
        Ok(statuses) => {
            for item in &prepared.tweet_relations {
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_media_ref",
                    statuses.get(&item.tweet_id).copied(),
                );
            }
        }
        Err(error) => {
            for item in &prepared.tweet_relations {
                prepared.tweet_results[item.index].failed("tweet_media_ref", error.to_string());
            }
        }
    }

    let mention_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.mention_refs.iter().cloned())
        .collect::<Vec<_>>();
    match store
        .sync_tweet_mention_refs(&tweet_ids, &mention_refs)
        .await
    {
        Ok(statuses) => {
            for item in &prepared.tweet_relations {
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_mention_ref",
                    statuses.get(&item.tweet_id).copied(),
                );
            }
        }
        Err(error) => {
            for item in &prepared.tweet_relations {
                prepared.tweet_results[item.index].failed("tweet_mention_ref", error.to_string());
            }
        }
    }

    let hashtag_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.hashtag_refs.iter().cloned())
        .collect::<Vec<_>>();
    match store
        .sync_tweet_hashtag_refs(&tweet_ids, &hashtag_refs)
        .await
    {
        Ok(statuses) => {
            for item in &prepared.tweet_relations {
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_hashtag_ref",
                    statuses.get(&item.tweet_id).copied(),
                );
            }
        }
        Err(error) => {
            for item in &prepared.tweet_relations {
                prepared.tweet_results[item.index].failed("tweet_hashtag_ref", error.to_string());
            }
        }
    }

    let symbol_refs = prepared
        .tweet_relations
        .iter()
        .flat_map(|item| item.symbol_refs.iter().cloned())
        .collect::<Vec<_>>();
    match store.sync_tweet_symbol_refs(&tweet_ids, &symbol_refs).await {
        Ok(statuses) => {
            for item in &prepared.tweet_relations {
                record_relation_sync(
                    &mut prepared.tweet_results[item.index],
                    "tweet_symbol_ref",
                    statuses.get(&item.tweet_id).copied(),
                );
            }
        }
        Err(error) => {
            for item in &prepared.tweet_relations {
                prepared.tweet_results[item.index].failed("tweet_symbol_ref", error.to_string());
            }
        }
    }
}

struct ConvertedTweet {
    tweet: Tweet,
    edit: Option<TweetEdit>,
    policy: Option<TweetPolicy>,
    stats: Option<TweetStats>,
    community_note: Option<TweetCommunityNote>,
    media_refs: Vec<TweetMediaRef>,
    mention_refs: Vec<TweetMentionRef>,
    hashtag_refs: Vec<TweetHashtagRef>,
    symbol_refs: Vec<TweetSymbolRef>,
}

async fn convert_user_snapshot(
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

fn convert_user_stats(
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

async fn convert_user_professional(
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

fn convert_user_identity(identity: Option<&SubmitUserIdentity>) -> AppResult<Option<UserIdentity>> {
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

fn convert_user_features(features: &SubmitUserFeatures) -> UserFeatures {
    UserFeatures {
        can_dm: features.can_dm,
        can_tag_media: features.can_tag_media,
        is_protected: features.is_protected,
        can_be_subscribed: features.can_be_subscribed,
    }
}

async fn convert_tweet(
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

fn convert_tweet_edit(tweet_id: i64, edit: &SubmitTweetEdit) -> AppResult<TweetEdit> {
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

fn convert_tweet_policy(tweet_id: i64, policy: &SubmitTweetPolicy) -> TweetPolicy {
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

fn convert_tweet_stats(
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

async fn convert_tweet_community_note(
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

fn convert_tweet_place(place: &SubmitTweetPlace) -> AppResult<Option<TweetPlace>> {
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

fn convert_media(media_id: i64, media: &SubmitMedia) -> AppResult<Media> {
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

fn convert_media_resource(
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

fn convert_media_geometry(geometry: &SubmitMediaGeometry) -> AppResult<MediaGeometry> {
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

fn convert_media_rect(rect: &SubmitMediaRect) -> AppResult<crate::tweet_model::MediaRect> {
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

fn convert_media_variants(variants: &SubmitMediaVariants) -> AppResult<MediaSizeVariants> {
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

fn convert_media_variant(variant: &SubmitMediaVariant) -> AppResult<MediaSizeVariant> {
    ensure_positive("media.variant.width", variant.width)?;
    ensure_positive("media.variant.height", variant.height)?;
    Ok(MediaSizeVariant {
        w: variant.width,
        h: variant.height,
        resize_mode: variant.resize_mode.clone(),
    })
}

fn convert_media_tag(tag: &SubmitMediaTag) -> AppResult<MediaTag> {
    Ok(MediaTag {
        user_id: parse_optional_i64_id(tag.user_id.as_deref(), "media.taggedUsers.userId")?,
        kind: tag.kind.clone(),
    })
}

fn convert_media_details(details: &SubmitMediaDetails) -> MediaDetails {
    MediaDetails {
        title: details.title.clone(),
        description: details.description.clone(),
        site_url: details.site_url.clone(),
        is_embeddable: details.is_embeddable,
        is_monetizable: details.is_monetizable,
    }
}

fn convert_media_video(video: &SubmitMediaVideo) -> AppResult<MediaVideo> {
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

fn convert_video_variant(variant: &SubmitVideoVariant) -> AppResult<VideoVariant> {
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

async fn convert_annotated_text(
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

fn convert_resolved_url(value: &SubmitResolvedUrl) -> ResolvedUrl {
    ResolvedUrl {
        url: value.url.clone(),
        expanded_url: value.expanded_url.clone(),
        display_text: value.display_text.clone(),
    }
}

fn collect_text_refs(
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

fn dedupe_refs(
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

fn dedupe_media_refs(media_refs: &mut Vec<TweetMediaRef>) {
    let mut seen = HashSet::new();
    media_refs.retain(|reference| seen.insert(reference.media_id));
}

fn collect_last_valid_i64_indices<T>(
    items: &[T],
    field: &str,
    id: impl Fn(&T) -> &str,
) -> HashMap<i64, usize> {
    let mut last = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let Ok(parsed) = parse_i64_id(id(item), field) {
            last.insert(parsed, index);
        }
    }
    last
}

fn record_conditional_write(
    result: &mut ObjectResultBuilder,
    name: &'static str,
    write: ConditionalWrite,
) {
    match write {
        ConditionalWrite::Inserted => result.accepted(name, "inserted"),
        ConditionalWrite::SkippedDuplicate => result.skipped(name, "duplicate_timestamp"),
        ConditionalWrite::SkippedUnchanged => result.skipped(name, "unchanged"),
        ConditionalWrite::SkippedInterval => result.skipped(name, "stats_interval_not_reached"),
        ConditionalWrite::SkippedMissingParent => result.skipped(name, "missing_parent"),
    }
}

fn summarize_results(response: &SubmitTweetResponse) -> SubmitSummary {
    let mut summary = SubmitSummary::default();
    for result in response
        .users
        .iter()
        .chain(response.tweets.iter())
        .chain(response.media.iter())
    {
        summary.total += 1;
        match result.status {
            SubmitObjectStatus::Accepted => summary.accepted += 1,
            SubmitObjectStatus::Skipped => summary.skipped += 1,
            SubmitObjectStatus::Partial => summary.partial += 1,
            SubmitObjectStatus::Failed => summary.failed += 1,
        }
    }
    summary
}

fn parse_i64_ids(values: &[String], field: &str) -> AppResult<Vec<i64>> {
    values
        .iter()
        .map(|value| parse_i64_id(value, field))
        .collect()
}

fn parse_i64_id(value: &str, field: &str) -> AppResult<i64> {
    value
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 64-bit integer")))
}

fn parse_i32_id(value: &str, field: &str) -> AppResult<i32> {
    value
        .parse::<i32>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 32-bit integer")))
}

fn parse_optional_i64_id(value: Option<&str>, field: &str) -> AppResult<Option<i64>> {
    value.map(|value| parse_i64_id(value, field)).transpose()
}

fn parse_optional_i64_string(value: Option<&str>, field: &str) -> AppResult<Option<i64>> {
    match value.map(str::trim) {
        Some("") | None => Ok(None),
        Some(value) => {
            let parsed = parse_i64_id(value, field)?;
            validate_optional_nonnegative(field, Some(parsed))
        }
    }
}

fn parse_optional_i32_string(value: Option<&str>, field: &str) -> AppResult<Option<i32>> {
    match value.map(str::trim) {
        Some("") | None => Ok(None),
        Some(value) => {
            let parsed = parse_i32_id(value, field)?;
            validate_optional_nonnegative(field, Some(parsed.into()))?
                .map(|value| {
                    value
                        .try_into()
                        .map_err(|_| AppError::bad_request(format!("{field} is too large")))
                })
                .transpose()
        }
    }
}

fn ensure_nonnegative_options(field: &str, values: &[Option<i64>]) -> AppResult<()> {
    for value in values.iter().flatten() {
        ensure_nonnegative(field, *value)?;
    }
    Ok(())
}

fn validate_optional_nonnegative(field: &str, value: Option<i64>) -> AppResult<Option<i64>> {
    if let Some(value) = value {
        ensure_nonnegative(field, value)?;
    }
    Ok(value)
}

fn ensure_nonnegative(field: &str, value: i64) -> AppResult<()> {
    if value < 0 {
        return Err(AppError::bad_request(format!(
            "{field} must be non-negative"
        )));
    }
    Ok(())
}

fn ensure_positive(field: &str, value: i32) -> AppResult<()> {
    if value <= 0 {
        return Err(AppError::bad_request(format!("{field} must be positive")));
    }
    Ok(())
}

fn validate_range(range: SubmitTextRange, field: &str) -> AppResult<SubmitTextRange> {
    ensure_nonnegative(&format!("{field}.start"), range.start.into())?;
    ensure_nonnegative(&format!("{field}.end"), range.end.into())?;
    if range.end < range.start {
        return Err(AppError::bad_request(format!(
            "{field}.end must be greater than or equal to {field}.start"
        )));
    }
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn submit_request_accepts_three_root_lists() {
        let request: SubmitTweetRequest = serde_json::from_value(json!({
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
    fn user_registered_at_accepts_legacy_created_at_alias() {
        let request: SubmitTweetRequest = serde_json::from_value(json!({
            "users": [{
                "id": "123",
                "createdAt": "2026-04-10T00:00:00Z"
            }]
        }))
        .unwrap();

        assert!(request.users[0].registered_at.is_some());
    }

    #[test]
    fn summary_counts_partial_results() {
        let response = SubmitTweetResponse {
            users: vec![SubmitObjectResult {
                id: Some("1".to_owned()),
                status: SubmitObjectStatus::Partial,
                operations: Vec::new(),
                error: None,
            }],
            ..Default::default()
        };

        let summary = summarize_results(&response);

        assert_eq!(summary.total, 1);
        assert_eq!(summary.partial, 1);
    }

    #[test]
    fn collect_last_valid_indices_prefers_last_duplicate() {
        let request: SubmitTweetRequest = serde_json::from_value(json!({
            "users": [
                {"id": "1"},
                {"id": "2"},
                {"id": "1"}
            ]
        }))
        .unwrap();

        let indices = collect_last_valid_i64_indices(&request.users, "user.id", |user| &user.id);

        assert_eq!(indices.get(&1), Some(&2));
        assert_eq!(indices.get(&2), Some(&1));
    }

    #[test]
    fn dedupe_media_refs_keeps_first_occurrence() {
        let mut refs = vec![
            TweetMediaRef {
                tweet_id: 1,
                media_id: 10,
                display_order: 0,
            },
            TweetMediaRef {
                tweet_id: 1,
                media_id: 10,
                display_order: 1,
            },
            TweetMediaRef {
                tweet_id: 1,
                media_id: 11,
                display_order: 2,
            },
        ];

        dedupe_media_refs(&mut refs);

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].media_id, 10);
        assert_eq!(refs[0].display_order, 0);
        assert_eq!(refs[1].media_id, 11);
    }
}
