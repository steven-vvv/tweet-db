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

mod batch;
mod contract;
mod convert;
mod execute;
mod handler;
mod prepare;
mod result;
#[cfg(test)]
mod tests;
mod validate;

pub use contract::*;
pub use handler::submit_tweets;

use self::{batch::*, convert::*, execute::*, prepare::*, result::*, validate::*};
