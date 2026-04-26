use super::*;

pub(in crate::tweet_query) async fn build_annotated_text_json(
    text: &DbAnnotatedText,
    hashtags: &HashMap<i32, Hashtag>,
    symbols: &HashMap<i32, Symbol>,
    string_dict: &StringDictCache,
) -> Result<Value, String> {
    let hashtags = text
        .hashtags
        .iter()
        .map(|reference| {
            let hashtag = hashtags
                .get(&reference.hashtag_id)
                .ok_or_else(|| format!("missing hashtag {}", reference.hashtag_id))?;
            Ok(json!({
                "text": hashtag.tag,
                "range": range_json(reference.range_start, reference.range_end),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let symbols = text
        .symbols
        .iter()
        .map(|reference| {
            let symbol = symbols
                .get(&reference.symbol_id)
                .ok_or_else(|| format!("missing symbol {}", reference.symbol_id))?;
            Ok(json!({
                "text": symbol.symbol,
                "range": range_json(reference.range_start, reference.range_end),
                "ticker": symbol.ticker,
                "name": symbol.name,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let urls = text
        .urls
        .iter()
        .map(|entity| {
            json!({
                "url": entity.url,
                "expandedUrl": entity.expanded_url,
                "displayText": entity.display_text,
                "range": range_json(entity.range_start, entity.range_end),
            })
        })
        .collect::<Vec<_>>();
    let mentions = text
        .mentions
        .iter()
        .map(|entity| {
            json!({
                "userId": entity.user_id.to_string(),
                "range": range_json(entity.range_start, entity.range_end),
            })
        })
        .collect::<Vec<_>>();
    let media = text
        .media_refs
        .iter()
        .map(media_entity_json)
        .collect::<Vec<_>>();
    let mut styles = Vec::with_capacity(text.styles.len());
    for style in &text.styles {
        styles.push(json!({
            "range": range_json(style.range_start, style.range_end),
            "styles": resolve_string_list(
                string_dict,
                &style.style_ids,
                StringSemantic::TweetTextStyleName,
                "text.styles",
            )
            .await?,
        }));
    }

    Ok(json!({
        "text": text.body,
        "displayRange": display_range_json(text.display_range_start, text.display_range_end),
        "entities": {
            "hashtags": hashtags,
            "symbols": symbols,
            "urls": urls,
            "mentions": mentions,
            "media": media,
        },
        "styles": styles,
    }))
}

pub(in crate::tweet_query) fn resolved_url_json(url: &DbResolvedUrl) -> Value {
    json!({
        "url": url.url,
        "expandedUrl": url.expanded_url,
        "displayText": url.display_text,
    })
}

pub(in crate::tweet_query) fn geo_point_json(point: &DbGeoPoint) -> Value {
    json!({
        "longitude": point.longitude,
        "latitude": point.latitude,
    })
}

pub(in crate::tweet_query) fn media_geometry_json(geometry: &DbMediaGeometry) -> Value {
    json!({
        "width": geometry.w,
        "height": geometry.h,
        "focusRects": geometry
            .focus_rects
            .iter()
            .map(|rect| {
                json!({
                    "x": rect.x,
                    "y": rect.y,
                    "width": rect.w,
                    "height": rect.h,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub(in crate::tweet_query) fn media_details_json(details: &DbMediaDetails) -> Value {
    json!({
        "title": details.title,
        "description": details.description,
        "siteUrl": details.site_url,
        "isEmbeddable": details.is_embeddable,
        "isMonetizable": details.is_monetizable,
    })
}

pub(in crate::tweet_query) fn media_entity_json(entity: &DbMediaEntity) -> Value {
    json!({
        "mediaId": entity.media_id.to_string(),
        "range": range_json(entity.range_start, entity.range_end),
        "displayText": empty_string_as_none(&entity.display_text),
        "expandedUrl": empty_string_as_none(&entity.expanded_url),
        "url": empty_string_as_none(&entity.url),
        "origin": media_origin_json(entity.origin_tweet_id, entity.origin_user_id),
    })
}

pub(in crate::tweet_query) fn media_origin_json(
    origin_tweet_id: Option<i64>,
    origin_user_id: Option<i64>,
) -> Option<Value> {
    (origin_tweet_id.is_some() || origin_user_id.is_some()).then(|| {
        json!({
            "tweetId": origin_tweet_id.map(|id| id.to_string()),
            "userId": origin_user_id.map(|id| id.to_string()),
        })
    })
}

pub(in crate::tweet_query) fn range_json(start: i32, end: i32) -> Value {
    json!({
        "start": start,
        "end": end,
    })
}

pub(in crate::tweet_query) fn display_range_json(
    start: Option<i32>,
    end: Option<i32>,
) -> Option<Value> {
    match (start, end) {
        (Some(start), Some(end)) => Some(range_json(start, end)),
        _ => None,
    }
}

pub(in crate::tweet_query) async fn resolve_optional_string(
    string_dict: &StringDictCache,
    id: Option<i16>,
    semantic: StringSemantic,
    field: &str,
) -> Result<Option<String>, String> {
    match id {
        Some(id) => resolve_string(string_dict, id, semantic, field)
            .await
            .map(Some),
        None => Ok(None),
    }
}

pub(in crate::tweet_query) async fn resolve_string(
    string_dict: &StringDictCache,
    id: i16,
    semantic: StringSemantic,
    field: &str,
) -> Result<String, String> {
    let entry = string_dict
        .get_entry(id)
        .await
        .ok_or_else(|| format!("missing string dictionary entry {id} for {field}"))?;
    ensure_semantic(&entry, semantic, field)?;
    Ok(entry.value)
}

pub(in crate::tweet_query) async fn resolve_string_list(
    string_dict: &StringDictCache,
    ids: &[i16],
    semantic: StringSemantic,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        values.push(resolve_string(string_dict, *id, semantic, field).await?);
    }
    Ok(values)
}

pub(in crate::tweet_query) fn ensure_semantic(
    entry: &StringDictValue,
    semantic: StringSemantic,
    field: &str,
) -> Result<(), String> {
    if entry.semantic == semantic {
        Ok(())
    } else {
        Err(format!(
            "dictionary semantic mismatch for {field}: expected {:?}, got {:?}",
            semantic, entry.semantic
        ))
    }
}

pub(in crate::tweet_query) fn collect_optional_annotated_text_lookup_ids(
    text: Option<&DbAnnotatedText>,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    if let Some(text) = text {
        collect_annotated_text_lookup_ids(text, hashtag_ids, symbol_ids);
    }
}

pub(in crate::tweet_query) fn collect_annotated_text_lookup_ids(
    text: &DbAnnotatedText,
    hashtag_ids: &mut HashSet<i32>,
    symbol_ids: &mut HashSet<i32>,
) {
    hashtag_ids.extend(text.hashtags.iter().map(|reference| reference.hashtag_id));
    symbol_ids.extend(text.symbols.iter().map(|reference| reference.symbol_id));
}
