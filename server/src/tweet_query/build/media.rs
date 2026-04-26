use super::*;

pub(in crate::tweet_query) async fn build_media_json(
    media: &DbMedia,
    resource: Option<&DbMediaResource>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let variants = match media.size_variants.as_ref() {
        Some(variants) => Some(build_media_variants_json(variants, string_dict).await?),
        None => None,
    };
    let tagged_users = build_media_tags_json(&media.tagged_users, string_dict).await?;
    let sensitivity_warnings = resolve_string_list(
        string_dict,
        &media.sensitivity_warning_ids,
        StringSemantic::TweetMediaSensitivityCode,
        "media.sensitivityWarnings",
    )
    .await?;
    let resource = match resource {
        Some(resource) => Some(build_media_resource_json(resource, string_dict).await?),
        None => None,
    };

    Ok(json!({
        "id": media.id.to_string(),
        "type": media.media_type,
        "altText": media.alt_text,
        "grokPostId": media.grok_post_id.map(|id| id.to_string()),
        "geometry": media.geometry.as_ref().map(media_geometry_json),
        "variants": variants,
        "taggedUsers": tagged_users,
        "sensitivityWarnings": sensitivity_warnings,
        "origin": media_origin_json(media.origin_tweet_id, media.origin_user_id),
        "details": media.details.as_ref().map(media_details_json),
        "resource": resource,
    }))
}

pub(in crate::tweet_query) async fn build_media_variants_json(
    variants: &DbMediaSizeVariants,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    Ok(json!({
        "large": match variants.large.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "medium": match variants.medium.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "small": match variants.small.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
        "thumb": match variants.thumb.as_ref() {
            Some(variant) => Some(build_media_variant_json(variant, string_dict).await?),
            None => None,
        },
    }))
}

pub(in crate::tweet_query) async fn build_media_variant_json(
    variant: &DbMediaSizeVariant,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let resize_mode = resolve_optional_string(
        string_dict,
        variant.resize_mode_id,
        StringSemantic::TweetMediaResizeMode,
        "media.variants.resizeMode",
    )
    .await?;

    Ok(json!({
        "width": variant.w,
        "height": variant.h,
        "resizeMode": resize_mode,
    }))
}

pub(in crate::tweet_query) async fn build_media_tags_json(
    tags: &[DbMediaTag],
    string_dict: &StringDictCache,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::with_capacity(tags.len());
    for tag in tags {
        let kind = resolve_optional_string(
            string_dict,
            tag.kind_id,
            StringSemantic::TweetMediaTagKind,
            "media.taggedUsers.kind",
        )
        .await?;
        values.push(json!({
            "userId": tag.user_id.map(|id| id.to_string()),
            "kind": kind,
        }));
    }
    Ok(values)
}

pub(in crate::tweet_query) async fn build_media_resource_json(
    resource: &DbMediaResource,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let availability = resolve_optional_string(
        string_dict,
        resource.availability_id,
        StringSemantic::TweetMediaAvailabilityStatus,
        "media.resource.availability",
    )
    .await?;
    let video = match resource.video.as_ref() {
        Some(video) => Some(build_media_video_json(video, string_dict).await?),
        None => None,
    };

    Ok(json!({
        "fetchedAt": format_time(resource.recorded_at),
        "mediaUrl": resource.media_url,
        "availability": availability,
        "video": video,
    }))
}

pub(in crate::tweet_query) async fn build_media_video_json(
    video: &DbMediaVideo,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let mut variants = Vec::with_capacity(video.variants.len());
    for variant in &video.variants {
        let content_type = resolve_optional_string(
            string_dict,
            variant.content_type_id,
            StringSemantic::TweetVideoContentType,
            "media.video.variants.contentType",
        )
        .await?;
        variants.push(json!({
            "contentType": content_type,
            "bitrate": variant.bitrate,
            "url": variant.url,
        }));
    }

    Ok(json!({
        "aspectRatio": match (video.aspect_ratio_w, video.aspect_ratio_h) {
            (Some(w), Some(h)) => Some([w, h]),
            _ => None::<[i32; 2]>,
        },
        "durationMs": video.duration_ms,
        "variants": variants,
    }))
}
