use crate::error::{AppError, AppResult};

pub const STORAGE_TODO_MESSAGE: &str =
    "storage subsystem is intentionally disabled during the tweet v2 bootstrap refactor";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSubsystemStatus {
    pub active: bool,
    pub message: &'static str,
}

pub fn status() -> StorageSubsystemStatus {
    StorageSubsystemStatus {
        active: false,
        message: STORAGE_TODO_MESSAGE,
    }
}

pub fn unavailable<T>() -> AppResult<T> {
    Err(AppError::bad_request(STORAGE_TODO_MESSAGE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_subsystem_reports_disabled_status() {
        let status = status();
        assert!(!status.active);
        assert_eq!(status.message, STORAGE_TODO_MESSAGE);
    }
}
