use super::*;

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
