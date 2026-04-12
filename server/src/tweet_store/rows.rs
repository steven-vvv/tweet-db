use super::*;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct UserStatsRow {
    pub(super) recorded_at: time::OffsetDateTime,
    pub(super) followers: Option<i64>,
    pub(super) following: Option<i64>,
    pub(super) likes: Option<i64>,
    pub(super) media_posts: Option<i64>,
    pub(super) tweets: Option<i64>,
    pub(super) listed: Option<i64>,
}

impl UserStatsRow {
    pub(super) fn same_user_stats(&self, value: &UserStats) -> bool {
        self.followers == value.followers
            && self.following == value.following
            && self.likes == value.likes
            && self.media_posts == value.media_posts
            && self.tweets == value.tweets
            && self.listed == value.listed
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TweetStatsRow {
    pub(super) recorded_at: time::OffsetDateTime,
    pub(super) views: Option<i64>,
    pub(super) replies: Option<i64>,
    pub(super) reposts: Option<i64>,
    pub(super) quotes: Option<i64>,
    pub(super) likes: Option<i64>,
    pub(super) bookmarks: Option<i64>,
}

impl TweetStatsRow {
    pub(super) fn same_tweet_stats(&self, value: &TweetStats) -> bool {
        self.views == value.views
            && self.replies == value.replies
            && self.reposts == value.reposts
            && self.quotes == value.quotes
            && self.likes == value.likes
            && self.bookmarks == value.bookmarks
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct JsonRowI64 {
    pub(super) id: i64,
    pub(super) data: Value,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct JsonRowString {
    pub(super) id: String,
    pub(super) data: Value,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TweetMediaRefRow {
    pub(super) tweet_id: i64,
    pub(super) media_id: i64,
    pub(super) display_order: i16,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct UserCategoryRow {
    pub(super) id: i16,
    pub(super) source_category_code: i32,
    pub(super) name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct HashtagRow {
    pub(super) id: i32,
    pub(super) tag: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SymbolRow {
    pub(super) id: i32,
    pub(super) symbol: String,
    pub(super) ticker: Option<String>,
    pub(super) name: Option<String>,
}

pub(super) fn conditional_write_from_db(value: &str) -> AppResult<ConditionalWrite> {
    match value {
        "inserted" => Ok(ConditionalWrite::Inserted),
        "duplicate" => Ok(ConditionalWrite::SkippedDuplicate),
        "unchanged" => Ok(ConditionalWrite::SkippedUnchanged),
        "interval" => Ok(ConditionalWrite::SkippedInterval),
        "missing_parent" => Ok(ConditionalWrite::SkippedMissingParent),
        other => Err(AppError::upstream(format!(
            "unexpected conditional write status: {other}"
        ))),
    }
}

pub(super) fn relation_sync_status_from_db(value: &str) -> AppResult<RelationSyncStatus> {
    match value {
        "replaced" => Ok(RelationSyncStatus::Replaced),
        "replaced_filtered" => Ok(RelationSyncStatus::ReplacedFiltered),
        "unchanged" => Ok(RelationSyncStatus::SkippedUnchanged),
        "unchanged_filtered" => Ok(RelationSyncStatus::SkippedUnchangedFiltered),
        "missing_tweet" => Ok(RelationSyncStatus::SkippedMissingTweet),
        other => Err(AppError::upstream(format!(
            "unexpected relation sync status: {other}"
        ))),
    }
}
