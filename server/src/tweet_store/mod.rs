use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::{
    error::{AppError, AppResult},
    string_dict::{StringDictCache, StringSemantic},
    tweet_model::{
        AnnotatedText, GeoPoint, Hashtag, HashtagRef, Media, MediaDetails, MediaEntity,
        MediaGeometry, MediaResource, MediaSizeVariant, MediaTag, MediaVideo, MentionEntity,
        ResolvedUrl, Symbol, SymbolRef, TextStyleRange, Tweet, TweetCommunityNote, TweetEdit,
        TweetHashtagRef, TweetMediaRef, TweetMentionRef, TweetPlace, TweetPolicy, TweetStats,
        TweetSymbolRef, TwitterUser, UrlEntity, UserCategory, UserDisclosure, UserFeatures,
        UserIdentity, UserProfessional, UserSnapshot, UserStats, UserVerification, VideoVariant,
    },
};

mod dicts;
mod lookup;
mod payloads;
mod read;
mod relations;
mod rows;
mod write_media;
mod write_tweets;
mod write_users;

use self::rows::*;

pub struct TweetStore<'a> {
    pool: &'a PgPool,
    string_dict: &'a StringDictCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalWrite {
    Inserted,
    SkippedDuplicate,
    SkippedUnchanged,
    SkippedInterval,
    SkippedMissingParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationSyncStatus {
    Replaced,
    ReplacedFiltered,
    SkippedUnchanged,
    SkippedUnchangedFiltered,
    SkippedMissingTweet,
}

impl<'a> TweetStore<'a> {
    pub fn new(pool: &'a PgPool, string_dict: &'a StringDictCache) -> Self {
        Self { pool, string_dict }
    }
}
