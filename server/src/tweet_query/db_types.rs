use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct DbTwitterUser {
    pub(super) id: i64,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub(super) registered_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbResolvedUrl {
    pub(super) url: String,
    pub(super) expanded_url: String,
    pub(super) display_text: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbHashtagRef {
    pub(super) hashtag_id: i32,
    pub(super) range_start: i32,
    pub(super) range_end: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbSymbolRef {
    pub(super) symbol_id: i32,
    pub(super) range_start: i32,
    pub(super) range_end: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUrlEntity {
    pub(super) url: String,
    pub(super) expanded_url: String,
    pub(super) display_text: String,
    pub(super) range_start: i32,
    pub(super) range_end: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMentionEntity {
    pub(super) user_id: i64,
    pub(super) range_start: i32,
    pub(super) range_end: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaEntity {
    pub(super) media_id: i64,
    pub(super) range_start: i32,
    pub(super) range_end: i32,
    pub(super) display_text: String,
    pub(super) expanded_url: String,
    pub(super) url: String,
    pub(super) origin_tweet_id: Option<i64>,
    pub(super) origin_user_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTextStyleRange {
    pub(super) range_start: i32,
    pub(super) range_end: i32,
    #[serde(default)]
    pub(super) style_ids: Vec<i16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbAnnotatedText {
    pub(super) body: String,
    pub(super) display_range_start: Option<i32>,
    pub(super) display_range_end: Option<i32>,
    #[serde(default)]
    pub(super) hashtags: Vec<DbHashtagRef>,
    #[serde(default)]
    pub(super) symbols: Vec<DbSymbolRef>,
    #[serde(default)]
    pub(super) urls: Vec<DbUrlEntity>,
    #[serde(default)]
    pub(super) mentions: Vec<DbMentionEntity>,
    #[serde(default)]
    pub(super) media_refs: Vec<DbMediaEntity>,
    #[serde(default)]
    pub(super) styles: Vec<DbTextStyleRange>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserVerification {
    pub(super) is_blue_verified: Option<bool>,
    pub(super) verified_type_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserDisclosure {
    pub(super) relation_id: Option<i16>,
    pub(super) subject_id: Option<i64>,
    pub(super) subject_handle: Option<String>,
    pub(super) subject_name: Option<String>,
    pub(super) subject_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserIdentity {
    pub(super) verification: Option<DbUserVerification>,
    pub(super) disclosure: Option<DbUserDisclosure>,
    pub(super) parody_label_id: Option<i16>,
    pub(super) has_completed_new_account_review: Option<bool>,
    pub(super) is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserFeatures {
    pub(super) can_dm: Option<bool>,
    pub(super) can_tag_media: Option<bool>,
    pub(super) is_protected: Option<bool>,
    pub(super) can_be_subscribed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserProfessional {
    pub(super) professional_id: Option<i64>,
    pub(super) professional_type_id: Option<i16>,
    #[serde(default)]
    pub(super) category_ids: Vec<i16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserSnapshot {
    #[serde(with = "time::serde::rfc3339")]
    pub(super) recorded_at: OffsetDateTime,
    pub(super) display_name: String,
    pub(super) user_name: String,
    pub(super) avatar_url: Option<String>,
    pub(super) uses_default_avatar: Option<bool>,
    pub(super) avatar_shape_id: Option<i16>,
    pub(super) banner_url: Option<String>,
    pub(super) location: Option<String>,
    pub(super) bio: Option<DbAnnotatedText>,
    #[serde(default)]
    pub(super) profile_links: Vec<DbResolvedUrl>,
    pub(super) identity: Option<DbUserIdentity>,
    pub(super) features: Option<DbUserFeatures>,
    pub(super) professional: Option<DbUserProfessional>,
    #[serde(default)]
    pub(super) pinned_tweet_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbUserStats {
    #[serde(with = "time::serde::rfc3339")]
    pub(super) recorded_at: OffsetDateTime,
    pub(super) followers: Option<i64>,
    pub(super) following: Option<i64>,
    pub(super) likes: Option<i64>,
    pub(super) media_posts: Option<i64>,
    pub(super) tweets: Option<i64>,
    pub(super) listed: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbGeoPoint {
    pub(super) longitude: f64,
    pub(super) latitude: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweetPlace {
    pub(super) id: String,
    pub(super) name: Option<String>,
    pub(super) full_name: Option<String>,
    pub(super) country_id: Option<i16>,
    pub(super) country_code_id: Option<i16>,
    pub(super) kind_id: Option<i16>,
    pub(super) boundary: Option<Vec<DbGeoPoint>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweet {
    pub(super) id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) published_at: OffsetDateTime,
    pub(super) source_id: Option<i16>,
    pub(super) author_id: i64,
    pub(super) place_id: Option<String>,
    pub(super) legacy_text: DbAnnotatedText,
    pub(super) note_id: Option<String>,
    pub(super) note_text: Option<DbAnnotatedText>,
    pub(super) language_id: Option<i16>,
    pub(super) conversation_id: i64,
    pub(super) reply_to_tweet_id: Option<i64>,
    pub(super) reply_to_user_id: Option<i64>,
    pub(super) quote_tweet_id: Option<i64>,
    pub(super) quote_permalink: Option<DbResolvedUrl>,
    pub(super) repost_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweetEdit {
    #[serde(default)]
    pub(super) version_ids: Vec<i64>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub(super) editable_until: Option<OffsetDateTime>,
    pub(super) remaining_edits: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweetPolicy {
    pub(super) reply_policy_id: Option<i16>,
    pub(super) followers_only: Option<bool>,
    pub(super) is_possibly_sensitive: Option<bool>,
    #[serde(default)]
    pub(super) available_action_ids: Vec<i16>,
    pub(super) is_media_visibility_restricted: Option<bool>,
    pub(super) paid_promotion: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweetStats {
    #[serde(with = "time::serde::rfc3339")]
    pub(super) recorded_at: OffsetDateTime,
    pub(super) views: Option<i64>,
    pub(super) replies: Option<i64>,
    pub(super) reposts: Option<i64>,
    pub(super) quotes: Option<i64>,
    pub(super) likes: Option<i64>,
    pub(super) bookmarks: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbTweetCommunityNote {
    pub(super) note_id: Option<i64>,
    pub(super) title: Option<String>,
    pub(super) short_title: Option<String>,
    pub(super) subtitle: Option<DbAnnotatedText>,
    pub(super) footer: Option<DbAnnotatedText>,
    pub(super) destination_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaRect {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaGeometry {
    pub(super) w: i32,
    pub(super) h: i32,
    #[serde(default)]
    pub(super) focus_rects: Vec<DbMediaRect>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaSizeVariant {
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) resize_mode_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaSizeVariants {
    pub(super) large: Option<DbMediaSizeVariant>,
    pub(super) medium: Option<DbMediaSizeVariant>,
    pub(super) small: Option<DbMediaSizeVariant>,
    pub(super) thumb: Option<DbMediaSizeVariant>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaTag {
    pub(super) user_id: Option<i64>,
    pub(super) kind_id: Option<i16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaDetails {
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) site_url: Option<String>,
    pub(super) is_embeddable: Option<bool>,
    pub(super) is_monetizable: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbVideoVariant {
    pub(super) content_type_id: Option<i16>,
    pub(super) bitrate: Option<i32>,
    pub(super) url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaVideo {
    pub(super) aspect_ratio_w: Option<i32>,
    pub(super) aspect_ratio_h: Option<i32>,
    pub(super) duration_ms: Option<i64>,
    #[serde(default)]
    pub(super) variants: Vec<DbVideoVariant>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMedia {
    pub(super) id: i64,
    #[serde(rename = "type")]
    pub(super) media_type: String,
    pub(super) alt_text: Option<String>,
    pub(super) grok_post_id: Option<uuid::Uuid>,
    pub(super) geometry: Option<DbMediaGeometry>,
    pub(super) size_variants: Option<DbMediaSizeVariants>,
    #[serde(default)]
    pub(super) tagged_users: Vec<DbMediaTag>,
    #[serde(default)]
    pub(super) sensitivity_warning_ids: Vec<i16>,
    pub(super) origin_tweet_id: Option<i64>,
    pub(super) origin_user_id: Option<i64>,
    pub(super) details: Option<DbMediaDetails>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DbMediaResource {
    #[serde(with = "time::serde::rfc3339")]
    pub(super) recorded_at: OffsetDateTime,
    pub(super) media_url: Option<String>,
    pub(super) availability_id: Option<i16>,
    pub(super) video: Option<DbMediaVideo>,
}
