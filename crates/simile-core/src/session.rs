//! Host-independent stateful sessions. Baseline runs use a separate fresh runtime.
use simile_http::{HttpExchange, HttpOptions, RequestBody, RouteTable};
use simile_service::{Service, WorkLease};
use simile_stack::{ClockMode, ClockSnapshot, LogicalTime};

use crate::{Result, ops};

pub struct ServiceSession {
    pub(crate) artifact_bytes: Vec<u8>,
    pub(crate) service: Service,
    pub(crate) initial_clock: ClockSnapshot,
    pub(crate) routes: RouteTable,
    pub(crate) options: HttpOptions,
}

impl ServiceSession {
    /// Browser sessions default to manual time at the Unix epoch.
    pub fn new(bytes: &[u8]) -> Result<Self> {
        Self::with_options(
            bytes,
            ClockSnapshot {
                mode: ClockMode::Manual,
                now: LogicalTime::UNIX_EPOCH,
            },
            HttpOptions::default(),
        )
    }

    pub fn with_options(bytes: &[u8], clock: ClockSnapshot, options: HttpOptions) -> Result<Self> {
        let artifact = simile_artifact::decode(bytes)?;
        let routes = RouteTable::new(&artifact);
        let service = Service::memory_with_clock(artifact, clock)?;
        Ok(Self {
            artifact_bytes: bytes.to_vec(),
            service,
            initial_clock: clock,
            routes,
            options,
        })
    }

    #[must_use]
    pub fn artifact_bytes(&self) -> &[u8] {
        &self.artifact_bytes
    }

    #[must_use]
    pub fn initial_clock(&self) -> ClockSnapshot {
        self.initial_clock
    }

    #[must_use]
    pub fn service(&self) -> &Service {
        &self.service
    }

    pub fn handle_request(&self, request: simile_http::http::Request<RequestBody>) -> HttpExchange {
        simile_http::handle_request(&self.service, &self.routes, request, &self.options)
    }

    pub fn inspect(&self, request: &ops::StateRequest) -> Result<ops::StateReport> {
        ops::inspect_snapshot(self.service.snapshot()?, request)
    }

    pub fn advance(&self, milliseconds: u64) -> Result<simile_stack::AdvanceReport> {
        Ok(self.service.advance(milliseconds)?)
    }

    /// Claim ready work in the same order used by the native dispatcher.
    pub fn lease_ready_work(&self, limit: usize) -> Result<Vec<WorkLease>> {
        Ok(simile_http::lease_ready_work(&self.service, limit)?.ready)
    }
}
