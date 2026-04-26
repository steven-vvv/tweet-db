use super::*;

impl<'a> TweetStore<'a> {
    pub(super) async fn annotated_text_payload(
        &self,
        value: &AnnotatedText,
    ) -> AppResult<AnnotatedTextPayload> {
        let mut styles = Vec::with_capacity(value.styles.len());
        for style in &value.styles {
            styles.push(self.text_style_range_payload(style).await?);
        }

        Ok(AnnotatedTextPayload {
            body: value.body.clone(),
            display_range_start: value.display_range_start,
            display_range_end: value.display_range_end,
            hashtags: value.hashtags.clone(),
            symbols: value.symbols.clone(),
            urls: value.urls.clone(),
            mentions: value.mentions.clone(),
            media_refs: value.media_refs.clone(),
            styles,
        })
    }

    pub(super) async fn text_style_range_payload(
        &self,
        value: &TextStyleRange,
    ) -> AppResult<TextStyleRangePayload> {
        Ok(TextStyleRangePayload {
            range_start: value.range_start,
            range_end: value.range_end,
            style_ids: self
                .string_dict
                .ensure_ids(self.pool, StringSemantic::TweetTextStyleName, &value.styles)
                .await?,
        })
    }

    pub(super) async fn user_snapshot_payload(
        &self,
        value: &UserSnapshot,
    ) -> AppResult<UserSnapshotPayload> {
        Ok(UserSnapshotPayload {
            user_id: value.user_id,
            recorded_at: value.recorded_at,
            display_name: value.display_name.clone(),
            user_name: value.user_name.clone(),
            avatar_url: value.avatar_url.clone(),
            uses_default_avatar: value.uses_default_avatar,
            avatar_shape_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserAvatarShape,
                    value.avatar_shape.as_deref(),
                )
                .await?,
            banner_url: value.banner_url.clone(),
            location: value.location.clone(),
            bio: match value.bio.as_ref() {
                Some(bio) => Some(self.annotated_text_payload(bio).await?),
                None => None,
            },
            profile_links: value.profile_links.clone(),
            identity: match value.identity.as_ref() {
                Some(identity) => Some(self.user_identity_payload(identity).await?),
                None => None,
            },
            features: value.features.clone(),
            professional: match value.professional.as_ref() {
                Some(professional) => Some(self.user_professional_payload(professional).await?),
                None => None,
            },
            pinned_tweet_ids: value.pinned_tweet_ids.clone(),
        })
    }

    pub(super) async fn user_identity_payload(
        &self,
        value: &UserIdentity,
    ) -> AppResult<UserIdentityPayload> {
        Ok(UserIdentityPayload {
            verification: match value.verification.as_ref() {
                Some(verification) => Some(self.user_verification_payload(verification).await?),
                None => None,
            },
            disclosure: match value.disclosure.as_ref() {
                Some(disclosure) => Some(self.user_disclosure_payload(disclosure).await?),
                None => None,
            },
            parody_label_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserParodyLabel,
                    value.parody_label.as_deref(),
                )
                .await?,
            has_completed_new_account_review: value.has_completed_new_account_review,
            is_possibly_sensitive: value.is_possibly_sensitive,
        })
    }

    pub(super) async fn user_verification_payload(
        &self,
        value: &UserVerification,
    ) -> AppResult<UserVerificationPayload> {
        Ok(UserVerificationPayload {
            is_blue_verified: value.is_blue_verified,
            verified_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserVerificationType,
                    value.verified_type.as_deref(),
                )
                .await?,
        })
    }

    pub(super) async fn user_disclosure_payload(
        &self,
        value: &UserDisclosure,
    ) -> AppResult<UserDisclosurePayload> {
        Ok(UserDisclosurePayload {
            relation_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserDisclosureRelation,
                    value.relation.as_deref(),
                )
                .await?,
            subject_id: value.subject_id,
            subject_handle: value.subject_handle.clone(),
            subject_name: value.subject_name.clone(),
            subject_url: value.subject_url.clone(),
        })
    }

    pub(super) async fn user_professional_payload(
        &self,
        value: &UserProfessional,
    ) -> AppResult<UserProfessionalPayload> {
        Ok(UserProfessionalPayload {
            professional_id: value.professional_id,
            professional_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetUserProfessionalType,
                    value.professional_type.as_deref(),
                )
                .await?,
            category_ids: value.category_ids.clone(),
        })
    }

    pub(super) async fn tweet_place_payload(
        &self,
        value: &TweetPlace,
    ) -> AppResult<TweetPlacePayload> {
        Ok(TweetPlacePayload {
            id: value.id.clone(),
            name: value.name.clone(),
            full_name: value.full_name.clone(),
            country_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetCountryName,
                    value.country.as_deref(),
                )
                .await?,
            country_code_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetCountryCode,
                    value.country_code.as_deref(),
                )
                .await?,
            kind_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetPlaceKind,
                    value.kind.as_deref(),
                )
                .await?,
            boundary: value.boundary.clone(),
        })
    }

    pub(super) async fn tweet_payload(&self, value: &Tweet) -> AppResult<TweetPayload> {
        Ok(TweetPayload {
            id: value.id,
            published_at: value.published_at,
            source_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetSource,
                    value.source.as_deref(),
                )
                .await?,
            author_id: value.author_id,
            place_id: value.place_id.clone(),
            legacy_text: self.annotated_text_payload(&value.legacy_text).await?,
            note_id: value.note_id.clone(),
            note_text: match value.note_text.as_ref() {
                Some(note_text) => Some(self.annotated_text_payload(note_text).await?),
                None => None,
            },
            language_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetLanguageCode,
                    value.language.as_deref(),
                )
                .await?,
            conversation_id: value.conversation_id,
            reply_to_tweet_id: value.reply_to_tweet_id,
            reply_to_user_id: value.reply_to_user_id,
            quote_tweet_id: value.quote_tweet_id,
            quote_permalink: value.quote_permalink.clone(),
            repost_id: value.repost_id,
        })
    }

    pub(super) async fn tweet_policy_payload(
        &self,
        value: &TweetPolicy,
    ) -> AppResult<TweetPolicyPayload> {
        Ok(TweetPolicyPayload {
            tweet_id: value.tweet_id,
            reply_policy_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetReplyPolicyCode,
                    value.reply_policy.as_deref(),
                )
                .await?,
            followers_only: value.followers_only,
            is_possibly_sensitive: value.is_possibly_sensitive,
            available_action_ids: self
                .string_dict
                .ensure_ids(
                    self.pool,
                    StringSemantic::TweetActionCode,
                    &value.available_actions,
                )
                .await?,
            is_media_visibility_restricted: value.is_media_visibility_restricted,
            paid_promotion: value.paid_promotion,
        })
    }

    pub(super) async fn tweet_community_note_payload(
        &self,
        value: &TweetCommunityNote,
    ) -> AppResult<TweetCommunityNotePayload> {
        Ok(TweetCommunityNotePayload {
            tweet_id: value.tweet_id,
            note_id: value.note_id,
            title: value.title.clone(),
            short_title: value.short_title.clone(),
            subtitle: match value.subtitle.as_ref() {
                Some(subtitle) => Some(self.annotated_text_payload(subtitle).await?),
                None => None,
            },
            footer: match value.footer.as_ref() {
                Some(footer) => Some(self.annotated_text_payload(footer).await?),
                None => None,
            },
            destination_url: value.destination_url.clone(),
        })
    }

    pub(super) async fn media_payload(&self, value: &Media) -> AppResult<MediaPayload> {
        Ok(MediaPayload {
            id: value.id,
            media_type: value.media_type.as_db_str().to_owned(),
            alt_text: value.alt_text.clone(),
            grok_post_id: value.grok_post_id,
            geometry: value.geometry.clone(),
            size_variants: match value.size_variants.as_ref() {
                Some(size_variants) => Some(self.media_size_variants_payload(size_variants).await?),
                None => None,
            },
            tagged_users: self.media_tag_payloads(&value.tagged_users).await?,
            sensitivity_warning_ids: self
                .string_dict
                .ensure_ids(
                    self.pool,
                    StringSemantic::TweetMediaSensitivityCode,
                    &value.sensitivity_warnings,
                )
                .await?,
            origin_tweet_id: value.origin_tweet_id,
            origin_user_id: value.origin_user_id,
            details: value.details.clone(),
        })
    }

    pub(super) async fn media_size_variants_payload(
        &self,
        value: &crate::tweet_model::MediaSizeVariants,
    ) -> AppResult<MediaSizeVariantsPayload> {
        Ok(MediaSizeVariantsPayload {
            large: self
                .optional_media_size_variant_payload(value.large.as_ref())
                .await?,
            medium: self
                .optional_media_size_variant_payload(value.medium.as_ref())
                .await?,
            small: self
                .optional_media_size_variant_payload(value.small.as_ref())
                .await?,
            thumb: self
                .optional_media_size_variant_payload(value.thumb.as_ref())
                .await?,
        })
    }

    pub(super) async fn optional_media_size_variant_payload(
        &self,
        value: Option<&MediaSizeVariant>,
    ) -> AppResult<Option<MediaSizeVariantPayload>> {
        match value {
            Some(value) => Ok(Some(MediaSizeVariantPayload {
                w: value.w,
                h: value.h,
                resize_mode_id: self
                    .string_dict
                    .ensure_id(
                        self.pool,
                        StringSemantic::TweetMediaResizeMode,
                        Some(&value.resize_mode),
                    )
                    .await?,
            })),
            None => Ok(None),
        }
    }

    pub(super) async fn media_tag_payloads(
        &self,
        value: &[MediaTag],
    ) -> AppResult<Vec<MediaTagPayload>> {
        let mut tags = Vec::with_capacity(value.len());
        for tag in value {
            tags.push(MediaTagPayload {
                user_id: tag.user_id,
                kind_id: self
                    .string_dict
                    .ensure_id(
                        self.pool,
                        StringSemantic::TweetMediaTagKind,
                        tag.kind.as_deref(),
                    )
                    .await?,
            });
        }
        Ok(tags)
    }

    pub(super) async fn media_resource_payload(
        &self,
        value: &MediaResource,
    ) -> AppResult<MediaResourcePayload> {
        Ok(MediaResourcePayload {
            media_id: value.media_id,
            recorded_at: value.recorded_at,
            media_url: value.media_url.clone(),
            availability_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetMediaAvailabilityStatus,
                    value.availability.as_deref(),
                )
                .await?,
            video: match value.video.as_ref() {
                Some(video) => Some(self.media_video_payload(video).await?),
                None => None,
            },
        })
    }

    pub(super) async fn media_video_payload(
        &self,
        value: &MediaVideo,
    ) -> AppResult<MediaVideoPayload> {
        let mut variants = Vec::with_capacity(value.variants.len());
        for variant in &value.variants {
            variants.push(self.video_variant_payload(variant).await?);
        }

        Ok(MediaVideoPayload {
            aspect_ratio_w: value.aspect_ratio_w,
            aspect_ratio_h: value.aspect_ratio_h,
            duration_ms: value.duration_ms,
            variants,
        })
    }

    pub(super) async fn video_variant_payload(
        &self,
        value: &VideoVariant,
    ) -> AppResult<VideoVariantPayload> {
        Ok(VideoVariantPayload {
            content_type_id: self
                .string_dict
                .ensure_id(
                    self.pool,
                    StringSemantic::TweetVideoContentType,
                    Some(&value.content_type),
                )
                .await?,
            bitrate: value.bitrate,
            url: value.url.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TextStyleRangePayload {
    range_start: i32,
    range_end: i32,
    style_ids: Vec<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AnnotatedTextPayload {
    body: String,
    display_range_start: Option<i32>,
    display_range_end: Option<i32>,
    hashtags: Vec<HashtagRef>,
    symbols: Vec<SymbolRef>,
    urls: Vec<UrlEntity>,
    mentions: Vec<MentionEntity>,
    media_refs: Vec<MediaEntity>,
    styles: Vec<TextStyleRangePayload>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct UserVerificationPayload {
    is_blue_verified: Option<bool>,
    verified_type_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct UserDisclosurePayload {
    relation_id: Option<i16>,
    subject_id: Option<i64>,
    subject_handle: Option<String>,
    subject_name: Option<String>,
    subject_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct UserIdentityPayload {
    verification: Option<UserVerificationPayload>,
    disclosure: Option<UserDisclosurePayload>,
    parody_label_id: Option<i16>,
    has_completed_new_account_review: Option<bool>,
    is_possibly_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct UserProfessionalPayload {
    professional_id: Option<i64>,
    professional_type_id: Option<i16>,
    category_ids: Vec<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct UserSnapshotPayload {
    user_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: time::OffsetDateTime,
    display_name: String,
    user_name: String,
    avatar_url: Option<String>,
    uses_default_avatar: Option<bool>,
    avatar_shape_id: Option<i16>,
    banner_url: Option<String>,
    location: Option<String>,
    bio: Option<AnnotatedTextPayload>,
    profile_links: Vec<ResolvedUrl>,
    identity: Option<UserIdentityPayload>,
    features: Option<UserFeatures>,
    professional: Option<UserProfessionalPayload>,
    pinned_tweet_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TweetPlacePayload {
    id: String,
    name: Option<String>,
    full_name: Option<String>,
    country_id: Option<i16>,
    country_code_id: Option<i16>,
    kind_id: Option<i16>,
    boundary: Option<Vec<GeoPoint>>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TweetPayload {
    id: i64,
    #[serde(with = "time::serde::rfc3339")]
    published_at: time::OffsetDateTime,
    source_id: Option<i16>,
    author_id: i64,
    place_id: Option<String>,
    legacy_text: AnnotatedTextPayload,
    note_id: Option<String>,
    note_text: Option<AnnotatedTextPayload>,
    language_id: Option<i16>,
    conversation_id: i64,
    reply_to_tweet_id: Option<i64>,
    reply_to_user_id: Option<i64>,
    quote_tweet_id: Option<i64>,
    quote_permalink: Option<ResolvedUrl>,
    repost_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TweetPolicyPayload {
    tweet_id: i64,
    reply_policy_id: Option<i16>,
    followers_only: Option<bool>,
    is_possibly_sensitive: Option<bool>,
    available_action_ids: Vec<i16>,
    is_media_visibility_restricted: Option<bool>,
    paid_promotion: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TweetCommunityNotePayload {
    tweet_id: i64,
    note_id: Option<i64>,
    title: Option<String>,
    short_title: Option<String>,
    subtitle: Option<AnnotatedTextPayload>,
    footer: Option<AnnotatedTextPayload>,
    destination_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaSizeVariantPayload {
    w: i32,
    h: i32,
    resize_mode_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaSizeVariantsPayload {
    large: Option<MediaSizeVariantPayload>,
    medium: Option<MediaSizeVariantPayload>,
    small: Option<MediaSizeVariantPayload>,
    thumb: Option<MediaSizeVariantPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaTagPayload {
    user_id: Option<i64>,
    kind_id: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaPayload {
    id: i64,
    media_type: String,
    alt_text: Option<String>,
    grok_post_id: Option<uuid::Uuid>,
    geometry: Option<MediaGeometry>,
    size_variants: Option<MediaSizeVariantsPayload>,
    tagged_users: Vec<MediaTagPayload>,
    sensitivity_warning_ids: Vec<i16>,
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
    details: Option<MediaDetails>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct VideoVariantPayload {
    content_type_id: Option<i16>,
    bitrate: Option<i32>,
    url: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaVideoPayload {
    aspect_ratio_w: Option<i32>,
    aspect_ratio_h: Option<i32>,
    duration_ms: Option<i64>,
    variants: Vec<VideoVariantPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MediaResourcePayload {
    media_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: time::OffsetDateTime,
    media_url: Option<String>,
    availability_id: Option<i16>,
    video: Option<MediaVideoPayload>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp() -> time::OffsetDateTime {
        time::OffsetDateTime::from_unix_timestamp(0).unwrap()
    }

    fn annotated_text_payload() -> AnnotatedTextPayload {
        AnnotatedTextPayload {
            body: "hello".to_owned(),
            display_range_start: None,
            display_range_end: None,
            hashtags: Vec::new(),
            symbols: Vec::new(),
            urls: Vec::new(),
            mentions: Vec::new(),
            media_refs: Vec::new(),
            styles: Vec::new(),
        }
    }

    #[test]
    fn timestamp_payload_fields_serialize_as_rfc3339_strings() {
        let user_snapshot = UserSnapshotPayload {
            user_id: 1,
            recorded_at: timestamp(),
            display_name: "User".to_owned(),
            user_name: "user".to_owned(),
            avatar_url: None,
            uses_default_avatar: None,
            avatar_shape_id: None,
            banner_url: None,
            location: None,
            bio: None,
            profile_links: Vec::new(),
            identity: None,
            features: None,
            professional: None,
            pinned_tweet_ids: Vec::new(),
        };
        let tweet = TweetPayload {
            id: 10,
            published_at: timestamp(),
            source_id: None,
            author_id: 1,
            place_id: None,
            legacy_text: annotated_text_payload(),
            note_id: None,
            note_text: None,
            language_id: None,
            conversation_id: 10,
            reply_to_tweet_id: None,
            reply_to_user_id: None,
            quote_tweet_id: None,
            quote_permalink: None,
            repost_id: None,
        };
        let media_resource = MediaResourcePayload {
            media_id: 20,
            recorded_at: timestamp(),
            media_url: None,
            availability_id: None,
            video: None,
        };

        assert_eq!(
            serde_json::to_value(user_snapshot).unwrap()["recorded_at"],
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            serde_json::to_value(tweet).unwrap()["published_at"],
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            serde_json::to_value(media_resource).unwrap()["recorded_at"],
            "1970-01-01T00:00:00Z"
        );
    }
}
