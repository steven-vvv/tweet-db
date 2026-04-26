use super::*;

pub(super) struct ObjectResultBuilder {
    id: Option<String>,
    operations: Vec<SubmitOperationResult>,
    fatal_error: Option<String>,
}

impl ObjectResultBuilder {
    pub(super) fn new(id: Option<String>) -> Self {
        Self {
            id,
            operations: Vec::new(),
            fatal_error: None,
        }
    }

    pub(super) fn accepted(&mut self, name: &'static str, reason: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Accepted, Some(reason.into()));
    }

    pub(super) fn skipped(&mut self, name: &'static str, reason: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Skipped, Some(reason.into()));
    }

    pub(super) fn failed(&mut self, name: &'static str, error: impl Into<String>) {
        self.operation(name, SubmitOperationStatus::Failed, Some(error.into()));
    }

    pub(super) fn fatal(&mut self, error: impl Into<String>) {
        self.fatal_error = Some(error.into());
    }

    pub(super) fn operation(
        &mut self,
        name: &'static str,
        status: SubmitOperationStatus,
        reason: Option<String>,
    ) {
        self.operations.push(SubmitOperationResult {
            name,
            status,
            reason,
        });
    }

    pub(super) fn finish(self) -> SubmitObjectResult {
        let has_accepted = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Accepted);
        let has_skipped = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Skipped);
        let has_failed = self
            .operations
            .iter()
            .any(|operation| operation.status == SubmitOperationStatus::Failed);

        let status = if has_failed && (has_accepted || has_skipped) {
            SubmitObjectStatus::Partial
        } else if has_failed || self.fatal_error.is_some() {
            SubmitObjectStatus::Failed
        } else if has_accepted {
            SubmitObjectStatus::Accepted
        } else {
            SubmitObjectStatus::Skipped
        };

        SubmitObjectResult {
            id: self.id,
            status,
            operations: self.operations,
            error: self.fatal_error,
        }
    }
}

pub(super) fn summarize_results(response: &SubmitTweetResponse) -> SubmitSummary {
    let mut summary = SubmitSummary::default();
    for result in response
        .users
        .iter()
        .chain(response.tweets.iter())
        .chain(response.media.iter())
    {
        summary.total += 1;
        match result.status {
            SubmitObjectStatus::Accepted => summary.accepted += 1,
            SubmitObjectStatus::Skipped => summary.skipped += 1,
            SubmitObjectStatus::Partial => summary.partial += 1,
            SubmitObjectStatus::Failed => summary.failed += 1,
        }
    }
    summary
}
