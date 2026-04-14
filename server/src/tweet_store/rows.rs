use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_write_status_mapping_covers_submit_statuses() {
        assert_eq!(
            conditional_write_from_db("inserted").unwrap(),
            ConditionalWrite::Inserted
        );
        assert_eq!(
            conditional_write_from_db("duplicate").unwrap(),
            ConditionalWrite::SkippedDuplicate
        );
        assert_eq!(
            conditional_write_from_db("unchanged").unwrap(),
            ConditionalWrite::SkippedUnchanged
        );
        assert_eq!(
            conditional_write_from_db("interval").unwrap(),
            ConditionalWrite::SkippedInterval
        );
        assert_eq!(
            conditional_write_from_db("missing_parent").unwrap(),
            ConditionalWrite::SkippedMissingParent
        );
    }

    #[test]
    fn relation_sync_status_mapping_covers_relation_statuses() {
        assert_eq!(
            relation_sync_status_from_db("replaced").unwrap(),
            RelationSyncStatus::Replaced
        );
        assert_eq!(
            relation_sync_status_from_db("replaced_filtered").unwrap(),
            RelationSyncStatus::ReplacedFiltered
        );
        assert_eq!(
            relation_sync_status_from_db("unchanged").unwrap(),
            RelationSyncStatus::SkippedUnchanged
        );
        assert_eq!(
            relation_sync_status_from_db("unchanged_filtered").unwrap(),
            RelationSyncStatus::SkippedUnchangedFiltered
        );
        assert_eq!(
            relation_sync_status_from_db("missing_tweet").unwrap(),
            RelationSyncStatus::SkippedMissingTweet
        );
    }
}
