use super::*;

pub(super) struct PreparedSubmitBatch {
    pub(super) user_results: Vec<ObjectResultBuilder>,
    pub(super) tweet_results: Vec<ObjectResultBuilder>,
    pub(super) media_results: Vec<ObjectResultBuilder>,
    pub(super) users: Vec<Indexed<TwitterUser>>,
    pub(super) tweet_authors: Vec<Indexed<TwitterUser>>,
    pub(super) user_snapshots: Vec<Indexed<UserSnapshot>>,
    pub(super) user_stats: Vec<Indexed<UserStats>>,
    pub(super) media: Vec<Indexed<Media>>,
    pub(super) media_resources: Vec<Indexed<MediaResource>>,
    pub(super) tweet_places: Vec<Indexed<TweetPlace>>,
    pub(super) tweets: Vec<Indexed<Tweet>>,
    pub(super) tweet_edits: Vec<Indexed<TweetEdit>>,
    pub(super) tweet_policies: Vec<Indexed<TweetPolicy>>,
    pub(super) tweet_community_notes: Vec<Indexed<TweetCommunityNote>>,
    pub(super) tweet_stats: Vec<Indexed<TweetStats>>,
    pub(super) tweet_relations: Vec<IndexedTweetRelations>,
}

impl PreparedSubmitBatch {
    pub(super) fn new(user_count: usize, tweet_count: usize, media_count: usize) -> Self {
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
pub(super) struct Indexed<T> {
    pub(super) index: usize,
    pub(super) value: T,
}

pub(super) struct IndexedTweetRelations {
    pub(super) index: usize,
    pub(super) tweet_id: i64,
    pub(super) media_refs: Vec<TweetMediaRef>,
    pub(super) mention_refs: Vec<TweetMentionRef>,
    pub(super) hashtag_refs: Vec<TweetHashtagRef>,
    pub(super) symbol_refs: Vec<TweetSymbolRef>,
}

pub(super) struct SubmitLookupIds {
    pub(super) user_categories: HashMap<i32, i16>,
    pub(super) hashtags: HashMap<String, i32>,
    pub(super) symbols: HashMap<String, i32>,
}

pub(super) struct ConvertedTweet {
    pub(super) tweet: Tweet,
    pub(super) edit: Option<TweetEdit>,
    pub(super) policy: Option<TweetPolicy>,
    pub(super) stats: Option<TweetStats>,
    pub(super) community_note: Option<TweetCommunityNote>,
    pub(super) media_refs: Vec<TweetMediaRef>,
    pub(super) mention_refs: Vec<TweetMentionRef>,
    pub(super) hashtag_refs: Vec<TweetHashtagRef>,
    pub(super) symbol_refs: Vec<TweetSymbolRef>,
}
