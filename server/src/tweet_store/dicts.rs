use super::*;

impl<'a> TweetStore<'a> {
    pub async fn preload_submit_batch_dicts(
        &self,
        snapshots: &[UserSnapshot],
        places: &[TweetPlace],
        tweets: &[Tweet],
        policies: &[TweetPolicy],
        notes: &[TweetCommunityNote],
        media: &[Media],
        resources: &[MediaResource],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_user_snapshot_dicts(&mut entries, snapshots);
        collect_tweet_place_dicts(&mut entries, places);
        collect_tweet_dicts(&mut entries, tweets);
        collect_tweet_policy_dicts(&mut entries, policies);
        collect_tweet_community_note_dicts(&mut entries, notes);
        collect_media_dicts(&mut entries, media);
        collect_media_resource_dicts(&mut entries, resources);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_dict_entries(
        &self,
        entries: Vec<(StringSemantic, String)>,
    ) -> AppResult<()> {
        self.string_dict.ensure_many(self.pool, entries).await
    }

    pub(super) async fn preload_user_snapshot_dicts(
        &self,
        snapshots: &[UserSnapshot],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_user_snapshot_dicts(&mut entries, snapshots);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_tweet_place_dicts(&self, places: &[TweetPlace]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_place_dicts(&mut entries, places);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_tweet_dicts(&self, tweets: &[Tweet]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_dicts(&mut entries, tweets);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_tweet_policy_dicts(
        &self,
        policies: &[TweetPolicy],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_policy_dicts(&mut entries, policies);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_tweet_community_note_dicts(
        &self,
        notes: &[TweetCommunityNote],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_tweet_community_note_dicts(&mut entries, notes);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_media_dicts(&self, media: &[Media]) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_media_dicts(&mut entries, media);
        self.preload_dict_entries(entries).await
    }

    pub(super) async fn preload_media_resource_dicts(
        &self,
        resources: &[MediaResource],
    ) -> AppResult<()> {
        let mut entries = Vec::new();
        collect_media_resource_dicts(&mut entries, resources);
        self.preload_dict_entries(entries).await
    }
}

fn push_optional_entry(
    entries: &mut Vec<(StringSemantic, String)>,
    semantic: StringSemantic,
    value: Option<&str>,
) {
    if let Some(value) = value {
        entries.push((semantic, value.to_owned()));
    }
}

fn collect_user_snapshot_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    snapshots: &[UserSnapshot],
) {
    for snapshot in snapshots {
        push_optional_entry(
            entries,
            StringSemantic::TweetUserAvatarShape,
            snapshot.avatar_shape.as_deref(),
        );
        if let Some(bio) = snapshot.bio.as_ref() {
            collect_annotated_text_dicts(entries, bio);
        }
        if let Some(identity) = snapshot.identity.as_ref() {
            if let Some(verification) = identity.verification.as_ref() {
                push_optional_entry(
                    entries,
                    StringSemantic::TweetUserVerificationType,
                    verification.verified_type.as_deref(),
                );
            }
            if let Some(disclosure) = identity.disclosure.as_ref() {
                push_optional_entry(
                    entries,
                    StringSemantic::TweetUserDisclosureRelation,
                    disclosure.relation.as_deref(),
                );
            }
            push_optional_entry(
                entries,
                StringSemantic::TweetUserParodyLabel,
                identity.parody_label.as_deref(),
            );
        }
        if let Some(professional) = snapshot.professional.as_ref() {
            push_optional_entry(
                entries,
                StringSemantic::TweetUserProfessionalType,
                professional.professional_type.as_deref(),
            );
        }
    }
}

fn collect_tweet_place_dicts(entries: &mut Vec<(StringSemantic, String)>, places: &[TweetPlace]) {
    for place in places {
        push_optional_entry(
            entries,
            StringSemantic::TweetCountryName,
            place.country.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetCountryCode,
            place.country_code.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetPlaceKind,
            place.kind.as_deref(),
        );
    }
}

fn collect_tweet_dicts(entries: &mut Vec<(StringSemantic, String)>, tweets: &[Tweet]) {
    for tweet in tweets {
        push_optional_entry(
            entries,
            StringSemantic::TweetSource,
            tweet.source.as_deref(),
        );
        push_optional_entry(
            entries,
            StringSemantic::TweetLanguageCode,
            tweet.language.as_deref(),
        );
        collect_annotated_text_dicts(entries, &tweet.legacy_text);
        if let Some(note_text) = tweet.note_text.as_ref() {
            collect_annotated_text_dicts(entries, note_text);
        }
    }
}

fn collect_tweet_policy_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    policies: &[TweetPolicy],
) {
    for policy in policies {
        push_optional_entry(
            entries,
            StringSemantic::TweetReplyPolicyCode,
            policy.reply_policy.as_deref(),
        );
        for action in &policy.available_actions {
            entries.push((StringSemantic::TweetActionCode, action.clone()));
        }
    }
}

fn collect_tweet_community_note_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    notes: &[TweetCommunityNote],
) {
    for note in notes {
        if let Some(subtitle) = note.subtitle.as_ref() {
            collect_annotated_text_dicts(entries, subtitle);
        }
        if let Some(footer) = note.footer.as_ref() {
            collect_annotated_text_dicts(entries, footer);
        }
    }
}

fn collect_media_dicts(entries: &mut Vec<(StringSemantic, String)>, media: &[Media]) {
    for item in media {
        if let Some(size_variants) = item.size_variants.as_ref() {
            collect_optional_media_size_variant_dicts(entries, size_variants.large.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.medium.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.small.as_ref());
            collect_optional_media_size_variant_dicts(entries, size_variants.thumb.as_ref());
        }
        for tag in &item.tagged_users {
            push_optional_entry(
                entries,
                StringSemantic::TweetMediaTagKind,
                tag.kind.as_deref(),
            );
        }
        for warning in &item.sensitivity_warnings {
            entries.push((StringSemantic::TweetMediaSensitivityCode, warning.clone()));
        }
    }
}

fn collect_media_resource_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    resources: &[MediaResource],
) {
    for resource in resources {
        push_optional_entry(
            entries,
            StringSemantic::TweetMediaAvailabilityStatus,
            resource.availability.as_deref(),
        );
        if let Some(video) = resource.video.as_ref() {
            collect_media_video_dicts(entries, video);
        }
    }
}

fn collect_annotated_text_dicts(entries: &mut Vec<(StringSemantic, String)>, text: &AnnotatedText) {
    for style in &text.styles {
        for name in &style.styles {
            entries.push((StringSemantic::TweetTextStyleName, name.clone()));
        }
    }
}

fn collect_optional_media_size_variant_dicts(
    entries: &mut Vec<(StringSemantic, String)>,
    variant: Option<&MediaSizeVariant>,
) {
    if let Some(variant) = variant {
        entries.push((
            StringSemantic::TweetMediaResizeMode,
            variant.resize_mode.clone(),
        ));
    }
}

fn collect_media_video_dicts(entries: &mut Vec<(StringSemantic, String)>, video: &MediaVideo) {
    for variant in &video.variants {
        entries.push((
            StringSemantic::TweetVideoContentType,
            variant.content_type.clone(),
        ));
    }
}
