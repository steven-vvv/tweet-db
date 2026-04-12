use super::*;

pub(super) fn summarize_results(response: &QueryTweetResponse) -> QuerySummary {
    let mut summary = QuerySummary::default();
    for result in response
        .users
        .iter()
        .chain(response.tweets.iter())
        .chain(response.media.iter())
    {
        summary.total += 1;
        match result.status {
            QueryObjectStatus::Found => summary.found += 1,
            QueryObjectStatus::Missing => summary.missing += 1,
            QueryObjectStatus::Failed => summary.failed += 1,
        }
    }
    summary
}

pub(super) fn found_result(id: Option<String>, data: Value) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Found,
        data: Some(data),
        error: None,
    }
}

pub(super) fn missing_result(id: Option<String>) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Missing,
        data: None,
        error: None,
    }
}

pub(super) fn failed_result(id: Option<String>, error: impl Into<String>) -> QueryObjectResult {
    QueryObjectResult {
        id,
        status: QueryObjectStatus::Failed,
        data: None,
        error: Some(error.into()),
    }
}

pub(super) fn selection_failed_or_missing_result(
    selection: QuerySelectionI64,
) -> QueryObjectResult {
    if let Some(error) = selection.error {
        failed_result(Some(selection.id), error)
    } else {
        missing_result(Some(selection.id))
    }
}

pub(super) fn parse_i64_selections(
    selectors: &[QueryIdSelector],
    field: &str,
) -> Vec<QuerySelectionI64> {
    selectors
        .iter()
        .map(|selector| match parse_i64_id(&selector.id, field) {
            Ok(parsed) => QuerySelectionI64 {
                id: selector.id.clone(),
                parsed: Some(parsed),
                error: None,
            },
            Err(error) => QuerySelectionI64 {
                id: selector.id.clone(),
                parsed: None,
                error: Some(error.to_string()),
            },
        })
        .collect()
}

pub(super) fn collect_unique_valid_ids(selections: &[QuerySelectionI64]) -> Vec<i64> {
    let mut seen = HashSet::new();
    selections
        .iter()
        .filter_map(|selection| selection.parsed)
        .filter(|id| seen.insert(*id))
        .collect()
}

pub(super) fn decode_i64_map<T: DeserializeOwned>(
    values: HashMap<i64, Value>,
    label: &str,
) -> HashMap<i64, Result<T, String>> {
    values
        .into_iter()
        .map(|(id, value)| {
            let decoded = serde_json::from_value(value)
                .map_err(|error| format!("failed to decode {label} {id}: {error}"));
            (id, decoded)
        })
        .collect()
}

pub(super) fn decode_string_map<T: DeserializeOwned>(
    values: HashMap<String, Value>,
    label: &str,
) -> HashMap<String, Result<T, String>> {
    values
        .into_iter()
        .map(|(id, value)| {
            let decoded = serde_json::from_value(value)
                .map_err(|error| format!("failed to decode {label} {id}: {error}"));
            (id, decoded)
        })
        .collect()
}

pub(super) fn decode_required<'a, K, T>(
    values: &'a HashMap<K, Result<T, String>>,
    key: &K,
) -> Option<Result<&'a T, String>>
where
    K: Eq + Hash,
{
    match values.get(key) {
        Some(Ok(value)) => Some(Ok(value)),
        Some(Err(error)) => Some(Err(error.clone())),
        None => None,
    }
}

pub(super) fn decode_optional<'a, K, T>(
    values: &'a HashMap<K, Result<T, String>>,
    key: &K,
) -> Result<Option<&'a T>, String>
where
    K: Eq + Hash,
{
    match values.get(key) {
        Some(Ok(value)) => Ok(Some(value)),
        Some(Err(error)) => Err(error.clone()),
        None => Ok(None),
    }
}

pub(super) fn parse_i64_id(value: &str, field: &str) -> AppResult<i64> {
    value
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 64-bit integer")))
}

pub(super) fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

pub(super) fn format_time_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_time)
}

pub(super) fn empty_string_as_none(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[derive(Debug)]
pub(super) struct QuerySelectionI64 {
    pub(super) id: String,
    pub(super) parsed: Option<i64>,
    pub(super) error: Option<String>,
}
