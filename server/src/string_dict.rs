use std::{collections::HashMap, sync::Arc};

use sqlx::{PgPool, Row};
use tokio::sync::RwLock;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StringSemantic {
    TweetTextStyleName,
    TweetUserVerificationType,
    TweetUserProfessionalType,
    TweetUserDisclosureRelation,
    TweetUserParodyLabel,
    TweetUserAvatarShape,
    TweetCountryName,
    TweetCountryCode,
    TweetLanguageCode,
    TweetMediaAvailabilityStatus,
    TweetMediaTagKind,
    TweetMediaResizeMode,
    TweetVideoContentType,
    TweetPlaceKind,
    TweetReplyPolicyCode,
    TweetActionCode,
    TweetSource,
}

impl StringSemantic {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::TweetTextStyleName => "tweet_text_style_name",
            Self::TweetUserVerificationType => "tweet_user_verification_type",
            Self::TweetUserProfessionalType => "tweet_user_professional_type",
            Self::TweetUserDisclosureRelation => "tweet_user_disclosure_relation",
            Self::TweetUserParodyLabel => "tweet_user_parody_label",
            Self::TweetUserAvatarShape => "tweet_user_avatar_shape",
            Self::TweetCountryName => "tweet_country_name",
            Self::TweetCountryCode => "tweet_country_code",
            Self::TweetLanguageCode => "tweet_language_code",
            Self::TweetMediaAvailabilityStatus => "tweet_media_availability_status",
            Self::TweetMediaTagKind => "tweet_media_tag_kind",
            Self::TweetMediaResizeMode => "tweet_media_resize_mode",
            Self::TweetVideoContentType => "tweet_video_content_type",
            Self::TweetPlaceKind => "tweet_place_kind",
            Self::TweetReplyPolicyCode => "tweet_reply_policy_code",
            Self::TweetActionCode => "tweet_action_code",
            Self::TweetSource => "tweet_source",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringDictValue {
    pub semantic: StringSemantic,
    pub value: String,
}

#[derive(Debug, Default)]
struct CacheState {
    by_key: HashMap<(StringSemantic, String), i16>,
    by_id: HashMap<i16, StringDictValue>,
}

#[derive(Debug, Clone, Default)]
pub struct StringDictCache {
    inner: Arc<RwLock<CacheState>>,
}

impl StringDictCache {
    pub async fn load(pool: &PgPool) -> AppResult<Self> {
        let cache = Self::default();
        cache.refresh(pool).await?;
        Ok(cache)
    }

    pub async fn refresh(&self, pool: &PgPool) -> AppResult<()> {
        let rows = sqlx::query(
            r#"
            SELECT id, semantic::text AS semantic, value
            FROM string_dict
            ORDER BY id ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        let mut next = CacheState::default();
        for row in rows {
            let id = row.get::<i16, _>("id");
            let semantic = semantic_from_db(row.get::<String, _>("semantic").as_str());
            let value = row.get::<String, _>("value");
            next.by_key.insert((semantic, value.clone()), id);
            next.by_id.insert(id, StringDictValue { semantic, value });
        }

        let mut guard = self.inner.write().await;
        *guard = next;
        Ok(())
    }

    pub async fn get_id(&self, semantic: StringSemantic, value: &str) -> Option<i16> {
        let value = normalize_value(value)?;
        let guard = self.inner.read().await;
        guard.by_key.get(&(semantic, value)).copied()
    }

    pub async fn get_value(&self, id: i16) -> Option<String> {
        let guard = self.inner.read().await;
        guard.by_id.get(&id).map(|entry| entry.value.clone())
    }

    pub async fn get_entry(&self, id: i16) -> Option<StringDictValue> {
        let guard = self.inner.read().await;
        guard.by_id.get(&id).cloned()
    }

    pub async fn ensure_id(
        &self,
        pool: &PgPool,
        semantic: StringSemantic,
        value: Option<&str>,
    ) -> AppResult<Option<i16>> {
        let Some(value) = value.and_then(normalize_value) else {
            return Ok(None);
        };

        if let Some(id) = self.get_id(semantic, &value).await {
            return Ok(Some(id));
        }

        let id =
            sqlx::query_scalar::<_, Option<i16>>("SELECT dict_id($1::string_semantic_enum, $2)")
                .bind(semantic.as_db_str())
                .bind(&value)
                .fetch_one(pool)
                .await?;

        if let Some(id) = id {
            self.insert_cached(id, semantic, value).await;
        }

        Ok(id)
    }

    pub async fn ensure_ids(
        &self,
        pool: &PgPool,
        semantic: StringSemantic,
        values: &[String],
    ) -> AppResult<Vec<i16>> {
        let mut ids = Vec::with_capacity(values.len());
        for value in values {
            if let Some(id) = self.ensure_id(pool, semantic, Some(value)).await? {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    async fn insert_cached(&self, id: i16, semantic: StringSemantic, value: String) {
        let mut guard = self.inner.write().await;
        guard.by_key.insert((semantic, value.clone()), id);
        guard.by_id.insert(id, StringDictValue { semantic, value });
    }
}

fn normalize_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn semantic_from_db(value: &str) -> StringSemantic {
    match value {
        "tweet_text_style_name" => StringSemantic::TweetTextStyleName,
        "tweet_user_verification_type" => StringSemantic::TweetUserVerificationType,
        "tweet_user_professional_type" => StringSemantic::TweetUserProfessionalType,
        "tweet_user_disclosure_relation" => StringSemantic::TweetUserDisclosureRelation,
        "tweet_user_parody_label" => StringSemantic::TweetUserParodyLabel,
        "tweet_user_avatar_shape" => StringSemantic::TweetUserAvatarShape,
        "tweet_country_name" => StringSemantic::TweetCountryName,
        "tweet_country_code" => StringSemantic::TweetCountryCode,
        "tweet_language_code" => StringSemantic::TweetLanguageCode,
        "tweet_media_availability_status" => StringSemantic::TweetMediaAvailabilityStatus,
        "tweet_media_tag_kind" => StringSemantic::TweetMediaTagKind,
        "tweet_media_resize_mode" => StringSemantic::TweetMediaResizeMode,
        "tweet_video_content_type" => StringSemantic::TweetVideoContentType,
        "tweet_place_kind" => StringSemantic::TweetPlaceKind,
        "tweet_reply_policy_code" => StringSemantic::TweetReplyPolicyCode,
        "tweet_action_code" => StringSemantic::TweetActionCode,
        "tweet_source" => StringSemantic::TweetSource,
        other => panic!("unknown string_dict semantic: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_tracks_both_directions() {
        let cache = StringDictCache::default();
        cache
            .insert_cached(7, StringSemantic::TweetSource, "web".to_owned())
            .await;

        assert_eq!(
            cache.get_id(StringSemantic::TweetSource, "web").await,
            Some(7)
        );
        assert_eq!(cache.get_value(7).await.as_deref(), Some("web"));

        let entry = cache.get_entry(7).await.unwrap();
        assert_eq!(entry.semantic, StringSemantic::TweetSource);
        assert_eq!(entry.value, "web");
    }

    #[test]
    fn semantic_names_match_schema_enum_values() {
        assert_eq!(
            StringSemantic::TweetMediaResizeMode.as_db_str(),
            "tweet_media_resize_mode"
        );
        assert_eq!(
            StringSemantic::TweetActionCode.as_db_str(),
            "tweet_action_code"
        );
    }

    #[test]
    fn normalize_value_rejects_empty_input() {
        assert_eq!(normalize_value("   "), None);
        assert_eq!(normalize_value(" web "), Some("web".to_owned()));
    }
}
