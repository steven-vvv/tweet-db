use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HashtagRef {
    pub hashtag_id: i32,
    pub range_start: i32,
    pub range_end: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SymbolRef {
    pub symbol_id: i32,
    pub range_start: i32,
    pub range_end: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResolvedUrl {
    pub url: String,
    pub expanded_url: String,
    pub display_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UrlEntity {
    pub url: String,
    pub expanded_url: String,
    pub display_text: String,
    pub range_start: i32,
    pub range_end: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MentionEntity {
    pub user_id: i64,
    pub range_start: i32,
    pub range_end: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaEntity {
    pub media_id: i64,
    pub range_start: i32,
    pub range_end: i32,
    pub display_text: String,
    pub expanded_url: String,
    pub url: String,
    pub origin_tweet_id: Option<i64>,
    pub origin_user_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TextStyleRange {
    pub range_start: i32,
    pub range_end: i32,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AnnotatedText {
    pub body: String,
    pub display_range_start: Option<i32>,
    pub display_range_end: Option<i32>,
    pub hashtags: Vec<HashtagRef>,
    pub symbols: Vec<SymbolRef>,
    pub urls: Vec<UrlEntity>,
    pub mentions: Vec<MentionEntity>,
    pub media_refs: Vec<MediaEntity>,
    pub styles: Vec<TextStyleRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserVerification {
    pub is_blue_verified: Option<bool>,
    pub verified_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserDisclosure {
    pub relation: Option<String>,
    pub subject_id: Option<i64>,
    pub subject_handle: Option<String>,
    pub subject_name: Option<String>,
    pub subject_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserIdentity {
    pub verification: Option<UserVerification>,
    pub disclosure: Option<UserDisclosure>,
    pub parody_label: Option<String>,
    pub has_completed_new_account_review: Option<bool>,
    pub is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserFeatures {
    pub can_dm: Option<bool>,
    pub can_tag_media: Option<bool>,
    pub is_protected: Option<bool>,
    pub can_be_subscribed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserProfessional {
    pub professional_id: Option<i64>,
    pub professional_type: Option<String>,
    pub category_ids: Vec<i16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GeoPoint {
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaSizeVariant {
    pub w: i32,
    pub h: i32,
    pub resize_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaSizeVariants {
    pub large: Option<MediaSizeVariant>,
    pub medium: Option<MediaSizeVariant>,
    pub small: Option<MediaSizeVariant>,
    pub thumb: Option<MediaSizeVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaGeometry {
    pub w: i32,
    pub h: i32,
    pub focus_rects: Vec<MediaRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaTag {
    pub user_id: Option<i64>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaDetails {
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_url: Option<String>,
    pub is_embeddable: Option<bool>,
    pub is_monetizable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VideoVariant {
    pub content_type: String,
    pub bitrate: Option<i32>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MediaVideo {
    pub aspect_ratio_w: Option<i32>,
    pub aspect_ratio_h: Option<i32>,
    pub duration_ms: Option<i64>,
    pub variants: Vec<VideoVariant>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    #[default]
    Photo,
    Video,
    AnimatedGif,
}

impl MediaType {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Video => "video",
            Self::AnimatedGif => "animated_gif",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TwitterUser {
    pub id: i64,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub registered_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserSnapshot {
    pub user_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    pub display_name: String,
    pub user_name: String,
    pub avatar_url: Option<String>,
    pub uses_default_avatar: Option<bool>,
    pub avatar_shape: Option<String>,
    pub banner_url: Option<String>,
    pub location: Option<String>,
    pub bio: Option<AnnotatedText>,
    pub profile_links: Vec<ResolvedUrl>,
    pub identity: Option<UserIdentity>,
    pub features: Option<UserFeatures>,
    pub professional: Option<UserProfessional>,
    pub pinned_tweet_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserStats {
    pub user_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    pub followers: Option<i64>,
    pub following: Option<i64>,
    pub likes: Option<i64>,
    pub media_posts: Option<i64>,
    pub tweets: Option<i64>,
    pub listed: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UserCategory {
    pub source_category_code: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Hashtag {
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Symbol {
    pub symbol: String,
    pub ticker: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TweetPlace {
    pub id: String,
    pub name: Option<String>,
    pub full_name: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub kind: Option<String>,
    pub boundary: Option<Vec<GeoPoint>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tweet {
    pub id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
    pub source: Option<String>,
    pub author_id: i64,
    pub place_id: Option<String>,
    pub legacy_text: AnnotatedText,
    pub note_id: Option<String>,
    pub note_text: Option<AnnotatedText>,
    pub language: Option<String>,
    pub conversation_id: i64,
    pub reply_to_tweet_id: Option<i64>,
    pub reply_to_user_id: Option<i64>,
    pub quote_tweet_id: Option<i64>,
    pub quote_permalink: Option<ResolvedUrl>,
    pub repost_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetEdit {
    pub tweet_id: i64,
    pub version_ids: Vec<i64>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub editable_until: Option<OffsetDateTime>,
    pub remaining_edits: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetPolicy {
    pub tweet_id: i64,
    pub reply_policy: Option<String>,
    pub followers_only: Option<bool>,
    pub is_possibly_sensitive: Option<bool>,
    pub available_actions: Vec<String>,
    pub is_media_visibility_restricted: Option<bool>,
    pub paid_promotion: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TweetStats {
    pub tweet_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    pub views: Option<i64>,
    pub replies: Option<i64>,
    pub reposts: Option<i64>,
    pub quotes: Option<i64>,
    pub likes: Option<i64>,
    pub bookmarks: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TweetCommunityNote {
    pub tweet_id: i64,
    pub note_id: Option<i64>,
    pub title: Option<String>,
    pub short_title: Option<String>,
    pub subtitle: Option<AnnotatedText>,
    pub footer: Option<AnnotatedText>,
    pub destination_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Media {
    pub id: i64,
    pub media_type: MediaType,
    pub alt_text: Option<String>,
    pub grok_post_id: Option<Uuid>,
    pub geometry: Option<MediaGeometry>,
    pub size_variants: Option<MediaSizeVariants>,
    pub tagged_users: Vec<MediaTag>,
    #[serde(default)]
    pub sensitivity_warnings: Vec<String>,
    pub origin_tweet_id: Option<i64>,
    pub origin_user_id: Option<i64>,
    pub details: Option<MediaDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaResource {
    pub media_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    pub media_url: Option<String>,
    pub availability: Option<String>,
    pub video: Option<MediaVideo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetMediaRef {
    pub tweet_id: i64,
    pub media_id: i64,
    pub display_order: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetMentionRef {
    pub tweet_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetHashtagRef {
    pub tweet_id: i64,
    pub hashtag_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TweetSymbolRef {
    pub tweet_id: i64,
    pub symbol_id: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn media_type_serializes_to_schema_values() {
        assert_eq!(
            serde_json::to_value(MediaType::AnimatedGif).unwrap(),
            json!("animated_gif")
        );
        assert_eq!(MediaType::Video.as_db_str(), "video");
    }

    #[test]
    fn annotated_text_keeps_snake_case_shape() {
        let value = serde_json::to_value(AnnotatedText {
            body: "hello".to_owned(),
            display_range_start: Some(0),
            display_range_end: Some(5),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(value["display_range_start"], 0);
        assert_eq!(value["body"], "hello");
    }
}
