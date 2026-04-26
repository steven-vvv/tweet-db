use super::*;

pub(in crate::tweet_submit) fn convert_media(
    media_id: i64,
    media: &SubmitMedia,
) -> AppResult<Media> {
    Ok(Media {
        id: media_id,
        media_type: match media.media_type {
            SubmitMediaType::Photo => MediaType::Photo,
            SubmitMediaType::Video => MediaType::Video,
            SubmitMediaType::AnimatedGif => MediaType::AnimatedGif,
        },
        alt_text: media.alt_text.clone(),
        grok_post_id: media
            .grok_post_id
            .as_deref()
            .map(|value| {
                Uuid::parse_str(value)
                    .map_err(|_| AppError::bad_request("media.grokPostId must be a UUID"))
            })
            .transpose()?,
        geometry: media
            .geometry
            .as_ref()
            .map(convert_media_geometry)
            .transpose()?,
        size_variants: media
            .variants
            .as_ref()
            .map(convert_media_variants)
            .transpose()?,
        tagged_users: media
            .tagged_users
            .iter()
            .map(convert_media_tag)
            .collect::<AppResult<_>>()?,
        sensitivity_warnings: media.sensitivity_warnings.clone(),
        origin_tweet_id: media
            .origin
            .as_ref()
            .and_then(|origin| origin.tweet_id.as_deref())
            .map(|id| parse_i64_id(id, "media.origin.tweetId"))
            .transpose()?,
        origin_user_id: media
            .origin
            .as_ref()
            .and_then(|origin| origin.user_id.as_deref())
            .map(|id| parse_i64_id(id, "media.origin.userId"))
            .transpose()?,
        details: media.details.as_ref().map(convert_media_details),
    })
}

pub(in crate::tweet_submit) fn convert_media_resource(
    media_id: i64,
    media: &SubmitMedia,
    now: OffsetDateTime,
) -> Option<AppResult<MediaResource>> {
    let resource = media.resource.as_ref();
    let media_url = resource
        .and_then(|resource| resource.media_url.clone())
        .or_else(|| media.media_url.clone());
    let availability = resource
        .and_then(|resource| resource.availability.clone())
        .or_else(|| media.availability.clone());
    let video = resource
        .and_then(|resource| resource.video.clone())
        .or_else(|| media.video.clone());

    if media_url.is_none() && availability.is_none() && video.is_none() {
        return None;
    }

    let video = match video.as_ref().map(convert_media_video).transpose() {
        Ok(video) => video,
        Err(error) => return Some(Err(error)),
    };

    Some(Ok(MediaResource {
        media_id,
        recorded_at: postgres_timestamptz(
            resource
                .and_then(|resource| resource.fetched_at)
                .or(media.fetched_at)
                .unwrap_or(now),
        ),
        media_url,
        availability,
        video,
    }))
}

pub(in crate::tweet_submit) fn convert_media_geometry(
    geometry: &SubmitMediaGeometry,
) -> AppResult<MediaGeometry> {
    ensure_positive("media.geometry.width", geometry.width)?;
    ensure_positive("media.geometry.height", geometry.height)?;
    Ok(MediaGeometry {
        w: geometry.width,
        h: geometry.height,
        focus_rects: geometry
            .focus_rects
            .iter()
            .map(convert_media_rect)
            .collect::<AppResult<_>>()?,
    })
}

pub(in crate::tweet_submit) fn convert_media_rect(
    rect: &SubmitMediaRect,
) -> AppResult<crate::tweet_model::MediaRect> {
    ensure_nonnegative("media.rect.x", rect.x.into())?;
    ensure_nonnegative("media.rect.y", rect.y.into())?;
    ensure_positive("media.rect.width", rect.width)?;
    ensure_positive("media.rect.height", rect.height)?;
    Ok(crate::tweet_model::MediaRect {
        x: rect.x,
        y: rect.y,
        w: rect.width,
        h: rect.height,
    })
}

pub(in crate::tweet_submit) fn convert_media_variants(
    variants: &SubmitMediaVariants,
) -> AppResult<MediaSizeVariants> {
    Ok(MediaSizeVariants {
        large: variants
            .large
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        medium: variants
            .medium
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        small: variants
            .small
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
        thumb: variants
            .thumb
            .as_ref()
            .map(convert_media_variant)
            .transpose()?,
    })
}

pub(in crate::tweet_submit) fn convert_media_variant(
    variant: &SubmitMediaVariant,
) -> AppResult<MediaSizeVariant> {
    ensure_positive("media.variant.width", variant.width)?;
    ensure_positive("media.variant.height", variant.height)?;
    Ok(MediaSizeVariant {
        w: variant.width,
        h: variant.height,
        resize_mode: variant.resize_mode.clone(),
    })
}

pub(in crate::tweet_submit) fn convert_media_tag(tag: &SubmitMediaTag) -> AppResult<MediaTag> {
    Ok(MediaTag {
        user_id: parse_optional_i64_id(tag.user_id.as_deref(), "media.taggedUsers.userId")?,
        kind: tag.kind.clone(),
    })
}

pub(in crate::tweet_submit) fn convert_media_details(details: &SubmitMediaDetails) -> MediaDetails {
    MediaDetails {
        title: details.title.clone(),
        description: details.description.clone(),
        site_url: details.site_url.clone(),
        is_embeddable: details.is_embeddable,
        is_monetizable: details.is_monetizable,
    }
}

pub(in crate::tweet_submit) fn convert_media_video(
    video: &SubmitMediaVideo,
) -> AppResult<MediaVideo> {
    if let Some([w, h]) = video.aspect_ratio {
        ensure_positive("media.video.aspectRatio[0]", w)?;
        ensure_positive("media.video.aspectRatio[1]", h)?;
    }
    let [aspect_ratio_w, aspect_ratio_h] = video.aspect_ratio.unwrap_or([0, 0]);
    Ok(MediaVideo {
        aspect_ratio_w: video.aspect_ratio.map(|_| aspect_ratio_w),
        aspect_ratio_h: video.aspect_ratio.map(|_| aspect_ratio_h),
        duration_ms: validate_optional_nonnegative("media.video.durationMs", video.duration_ms)?,
        variants: video
            .variants
            .iter()
            .map(convert_video_variant)
            .collect::<AppResult<_>>()?,
    })
}

pub(in crate::tweet_submit) fn convert_video_variant(
    variant: &SubmitVideoVariant,
) -> AppResult<VideoVariant> {
    Ok(VideoVariant {
        content_type: variant.content_type.clone(),
        bitrate: validate_optional_nonnegative(
            "media.video.variant.bitrate",
            variant.bitrate.map(i64::from),
        )?
        .map(|value| {
            value
                .try_into()
                .map_err(|_| AppError::bad_request("media.video.variant.bitrate is too large"))
        })
        .transpose()?,
        url: variant.url.clone(),
    })
}
