use super::*;

pub(super) fn collect_last_valid_i64_indices<T>(
    items: &[T],
    field: &str,
    id: impl Fn(&T) -> &str,
) -> HashMap<i64, usize> {
    let mut last = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let Ok(parsed) = parse_i64_id(id(item), field) {
            last.insert(parsed, index);
        }
    }
    last
}

pub(super) fn parse_i64_ids(values: &[String], field: &str) -> AppResult<Vec<i64>> {
    values
        .iter()
        .map(|value| parse_i64_id(value, field))
        .collect()
}

pub(super) fn parse_i64_id(value: &str, field: &str) -> AppResult<i64> {
    value
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 64-bit integer")))
}

pub(super) fn parse_i32_id(value: &str, field: &str) -> AppResult<i32> {
    value
        .parse::<i32>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a signed 32-bit integer")))
}

pub(super) fn parse_optional_i64_id(value: Option<&str>, field: &str) -> AppResult<Option<i64>> {
    value.map(|value| parse_i64_id(value, field)).transpose()
}

pub(super) fn parse_optional_i64_string(
    value: Option<&str>,
    field: &str,
) -> AppResult<Option<i64>> {
    match value.map(str::trim) {
        Some("") | None => Ok(None),
        Some(value) => {
            let parsed = parse_i64_id(value, field)?;
            validate_optional_nonnegative(field, Some(parsed))
        }
    }
}

pub(super) fn parse_optional_i32_string(
    value: Option<&str>,
    field: &str,
) -> AppResult<Option<i32>> {
    match value.map(str::trim) {
        Some("") | None => Ok(None),
        Some(value) => {
            let parsed = parse_i32_id(value, field)?;
            validate_optional_nonnegative(field, Some(parsed.into()))?
                .map(|value| {
                    value
                        .try_into()
                        .map_err(|_| AppError::bad_request(format!("{field} is too large")))
                })
                .transpose()
        }
    }
}

pub(super) fn ensure_nonnegative_options(field: &str, values: &[Option<i64>]) -> AppResult<()> {
    for value in values.iter().flatten() {
        ensure_nonnegative(field, *value)?;
    }
    Ok(())
}

pub(super) fn validate_optional_nonnegative(
    field: &str,
    value: Option<i64>,
) -> AppResult<Option<i64>> {
    if let Some(value) = value {
        ensure_nonnegative(field, value)?;
    }
    Ok(value)
}

pub(super) fn ensure_nonnegative(field: &str, value: i64) -> AppResult<()> {
    if value < 0 {
        return Err(AppError::bad_request(format!(
            "{field} must be non-negative"
        )));
    }
    Ok(())
}

pub(super) fn ensure_positive(field: &str, value: i32) -> AppResult<()> {
    if value <= 0 {
        return Err(AppError::bad_request(format!("{field} must be positive")));
    }
    Ok(())
}

pub(super) fn validate_range(range: SubmitTextRange, field: &str) -> AppResult<SubmitTextRange> {
    ensure_nonnegative(&format!("{field}.start"), range.start.into())?;
    ensure_nonnegative(&format!("{field}.end"), range.end.into())?;
    if range.end < range.start {
        return Err(AppError::bad_request(format!(
            "{field}.end must be greater than or equal to {field}.start"
        )));
    }
    Ok(range)
}
