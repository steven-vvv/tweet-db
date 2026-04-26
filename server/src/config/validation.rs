use std::{fs, path::Path};

use url::Url;

use crate::error::{AppError, AppResult};

pub(super) fn validate_url_scheme(name: &str, value: &str, expected_scheme: &str) -> AppResult<()> {
    let parsed =
        Url::parse(value).map_err(|error| AppError::config(format!("invalid {name}: {error}")))?;
    if parsed.scheme() != expected_scheme {
        return Err(AppError::config(format!(
            "{name} must use the {expected_scheme} scheme when server.mode = \"https\""
        )));
    }
    Ok(())
}

pub(super) fn ensure_file_declared(name: &str, path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        AppError::config(format!(
            "{name} could not be accessed at {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::config(format!(
            "{name} must reference a file: {}",
            path.display()
        )));
    }
    Ok(())
}
