use simile_spec::ResponseStatus;

use crate::{
    Error, ResponseSynthesisRequest, Result, Runtime, SynthesizedResponse, synthesize_response,
    synthesize_response_with_seed,
};

impl Runtime {
    /// Synthesize the first service response declared for one of `statuses`.
    ///
    /// Exact responses win over class responses such as `4XX`, and class
    /// responses win over `default`. Candidate order is retained within each
    /// tier. This lets a transport ask the emulated service to own a protocol
    /// failure without mistaking an unrelated success response for an error.
    pub fn synthesize_first_declared_response(
        &self,
        operation_key: &str,
        statuses: &[u16],
    ) -> Result<Option<SynthesizedResponse>> {
        let operation =
            self.artifact()
                .operation(operation_key)
                .ok_or_else(|| Error::UnknownOperation {
                    operation: operation_key.to_owned(),
                })?;
        let responses = operation.responses().collect::<Vec<_>>();
        let status = statuses
            .iter()
            .copied()
            .find(|status| {
                responses
                    .iter()
                    .any(|response| response.status() == &ResponseStatus::Exact(*status))
            })
            .or_else(|| {
                statuses.iter().copied().find(|status| {
                    responses.iter().any(|response| {
                        matches!(response.status(), ResponseStatus::Range(_))
                            && response.status().matches(*status)
                    })
                })
            })
            .or_else(|| {
                responses
                    .iter()
                    .any(|response| response.status() == &ResponseStatus::Default)
                    .then(|| statuses.first().copied())
                    .flatten()
            });
        let Some(status) = status else {
            return Ok(None);
        };
        synthesize_response(
            self.artifact(),
            ResponseSynthesisRequest {
                operation: operation_key,
                status: Some(status),
                content_type: None,
                request_body: None,
                request_parameters: None,
                body: None,
            },
        )
        .map(Some)
    }

    /// Render the service-declared response for a transport/runtime failure.
    pub fn synthesize_protocol_error(
        &self,
        operation_key: &str,
        cause: &str,
    ) -> Result<Option<SynthesizedResponse>> {
        self.synthesize_protocol_error_with_seed(operation_key, cause, rand::random())
    }

    /// Render a declared protocol error using the caller's deterministic seed.
    pub fn synthesize_protocol_error_with_seed(
        &self,
        operation_key: &str,
        cause: &str,
        seed: u64,
    ) -> Result<Option<SynthesizedResponse>> {
        let Some(mapping) = self.artifact().protocol_error(cause) else {
            return Ok(None);
        };
        let operation =
            self.artifact()
                .operation(operation_key)
                .ok_or_else(|| Error::UnknownOperation {
                    operation: operation_key.to_owned(),
                })?;
        let status = mapping.statuses().iter().copied().find(|status| {
            operation
                .responses()
                .any(|response| response.status().matches(*status))
        });
        let Some(status) = status else {
            return Ok(None);
        };
        synthesize_response_with_seed(
            self.artifact(),
            ResponseSynthesisRequest {
                operation: operation_key,
                status: Some(status),
                content_type: mapping.content_type(),
                request_body: None,
                request_parameters: None,
                body: mapping.body(),
            },
            seed,
        )
        .map(Some)
    }
}
