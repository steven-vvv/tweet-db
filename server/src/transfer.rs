use crate::error::{AppError, AppResult};

pub const TRANSFER_TODO_MESSAGE: &str =
    "transfer subsystem is intentionally disabled during the tweet v2 bootstrap refactor";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSubsystemStatus {
    pub active: bool,
    pub message: &'static str,
}

pub fn status() -> TransferSubsystemStatus {
    TransferSubsystemStatus {
        active: false,
        message: TRANSFER_TODO_MESSAGE,
    }
}

pub fn unavailable<T>() -> AppResult<T> {
    Err(AppError::bad_request(TRANSFER_TODO_MESSAGE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_subsystem_reports_disabled_status() {
        let status = status();
        assert!(!status.active);
        assert_eq!(status.message, TRANSFER_TODO_MESSAGE);
    }
}
