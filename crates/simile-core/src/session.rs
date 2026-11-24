//! Host-independent stateful sessions. Baseline runs use a separate fresh runtime.
use serde::{Deserialize, Serialize};
use simile_http::{HttpExchange, HttpOptions, RequestBody, RouteTable};
use simile_service::{Service, WorkLease};
use simile_stack::{ClockMode, ClockSnapshot, LogicalTime, StateStore};
use std::{cell::Cell, collections::BTreeMap, sync::Arc};

use crate::{Error, Result, ops};

pub struct ServiceSession {
    pub(crate) artifact_bytes: Vec<u8>,
    pub(crate) service: Service,
    pub(crate) initial_clock: ClockSnapshot,
    pub(crate) routes: RouteTable,
    pub(crate) options: HttpOptions,
    revision: Cell<u64>,
    generation: u64,
    instance: u128,
    next_handle: u64,
    handles: BTreeMap<String, WorkLease>,
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
        if options
            .scenario
            .as_deref()
            .is_some_and(|name| !artifact.has_scenario(name))
        {
            return Err(Error::Workspace("unknown scenario".to_owned()));
        }
        if options
            .fault_profile
            .as_deref()
            .is_some_and(|name| !artifact.has_fault_profile(name))
        {
            return Err(Error::Workspace("unknown fault profile".to_owned()));
        }
        let routes = RouteTable::new(&artifact);
        let service = Service::memory_with_clock(artifact, clock)?;
        Ok(Self {
            artifact_bytes: bytes.to_vec(),
            service,
            initial_clock: clock,
            routes,
            options,
            revision: Cell::new(0),
            generation: 0,
            instance: rand::random(),
            next_handle: 0,
            handles: BTreeMap::new(),
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
        self.changed();
        simile_http::handle_request(&self.service, &self.routes, request, &self.options)
    }

    pub fn handle_request_with_seed(
        &self,
        request: simile_http::http::Request<RequestBody>,
        response_seed: Option<u64>,
    ) -> HttpExchange {
        self.changed();
        let options = HttpOptions {
            response_seed: response_seed.or(self.options.response_seed),
            ..self.options.clone()
        };
        simile_http::handle_request(&self.service, &self.routes, request, &options)
    }

    pub fn inspect(&self, request: &ops::StateRequest) -> Result<ops::StateReport> {
        ops::inspect_snapshot(self.service.snapshot()?, request)
    }

    pub fn advance(&self, milliseconds: u64) -> Result<simile_stack::AdvanceReport> {
        let report = self.service.advance(milliseconds)?;
        self.changed();
        Ok(report)
    }

    /// Claim ready work in the same order used by the native dispatcher.
    pub fn lease_ready_work(&self, limit: usize) -> Result<Vec<WorkLease>> {
        let batch = simile_http::lease_ready_work(&self.service, limit)?;
        self.changed();
        Ok(batch.ready)
    }
}

const CHECKPOINT_VERSION: u16 = 1;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    version: u16,
    artifact_digest: String,
    artifact_format: u16,
    service: String,
    stack_protocol: u16,
    clock: ClockSnapshot,
    initial_clock: ClockSnapshot,
    session_revision: String,
    snapshot: simile_stack::StackSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub generation: String,
    pub revision: String,
    pub clock_mode: ClockMode,
    pub clock_milliseconds: String,
    pub artifact_digest: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub handle: String,
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub attempt: u32,
    pub work_json: String,
}

impl ServiceSession {
    fn changed(&self) {
        self.revision.set(self.revision.get().saturating_add(1));
    }

    pub fn status(&self) -> Result<SessionStatus> {
        let clock = self.service.clock()?;
        Ok(SessionStatus {
            generation: self.generation.to_string(),
            revision: self.revision.get().to_string(),
            clock_mode: clock.mode,
            clock_milliseconds: clock.now.as_unix_millis().to_string(),
            artifact_digest: self.service.artifact().summary().payload_sha256,
        })
    }

    /// Only a fully constructed candidate replaces the current runtime and state.
    pub fn reset(&mut self) -> Result<()> {
        let candidate = Service::memory_with_clock(
            simile_artifact::decode(&self.artifact_bytes)?,
            self.initial_clock,
        )?;
        self.replace(candidate)?;
        Ok(())
    }

    fn replace(&mut self, candidate: Service) -> Result<()> {
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error::Checkpoint("session generation overflow".to_owned()))?;
        self.service = candidate;
        self.generation = generation;
        self.handles.clear();
        self.changed();
        Ok(())
    }

    /// Capture canonical bytes at a completed operation boundary. This does not
    /// claim that any browser storage transaction has committed.
    pub fn export_checkpoint(&self) -> Result<Vec<u8>> {
        let snapshot = self.service.snapshot()?;
        let summary = self.service.artifact().summary();
        let checkpoint = Checkpoint {
            version: CHECKPOINT_VERSION,
            artifact_digest: summary.payload_sha256,
            artifact_format: summary.format_version,
            service: summary.service,
            stack_protocol: simile_stack::PROTOCOL_VERSION,
            clock: snapshot.clock(),
            initial_clock: self.initial_clock,
            session_revision: self.revision.get().to_string(),
            snapshot,
        };
        serde_json::to_vec(&checkpoint).map_err(|error| Error::Checkpoint(error.to_string()))
    }

    /// Validate everything before replacement. Stack state survives; runtime
    /// replay caches and fault RNG progress restart, like a native process restart.
    pub fn restore_checkpoint(&mut self, bytes: &[u8]) -> Result<()> {
        let checkpoint: Checkpoint =
            serde_json::from_slice(bytes).map_err(|error| Error::Checkpoint(error.to_string()))?;
        let summary = self.service.artifact().summary();
        if checkpoint.version != CHECKPOINT_VERSION
            || checkpoint.artifact_digest != summary.payload_sha256
            || checkpoint.artifact_format != summary.format_version
            || checkpoint.service != summary.service
            || checkpoint.stack_protocol != simile_stack::PROTOCOL_VERSION
            || checkpoint.clock != checkpoint.snapshot.clock()
            || checkpoint.initial_clock.mode != checkpoint.clock.mode
            || checkpoint.initial_clock != checkpoint.snapshot.initial_clock()
        {
            return Err(Error::Checkpoint(
                "format, artifact, service, protocol, or clock does not match".to_owned(),
            ));
        }
        checkpoint
            .session_revision
            .parse::<u64>()
            .map_err(|_| Error::Checkpoint("invalid captured revision".to_owned()))?;
        checkpoint.snapshot.validate().map_err(Error::Checkpoint)?;
        let restored = simile_stack::Stack::from_snapshot(checkpoint.snapshot.clone());
        let current = simile_stack::Stack::from_snapshot(self.service.snapshot()?);
        if restored.resources() != current.resources() {
            return Err(Error::Checkpoint("resource set does not match".to_owned()));
        }
        let store = Arc::new(simile_stack::MemoryStateStore::default());
        store.save(&checkpoint.snapshot)?;
        let candidate = Service::with_store_and_clock(
            simile_artifact::decode(&self.artifact_bytes)?,
            store,
            checkpoint.initial_clock,
        )?;
        self.replace(candidate)?;
        self.initial_clock = checkpoint.initial_clock;
        Ok(())
    }

    pub fn setup_profile(
        &self,
        principal: Option<&str>,
        name: Option<&str>,
        values: BTreeMap<String, serde_json::Value>,
        select: bool,
        seed: u64,
    ) -> Result<simile_service::SetupProfileResult> {
        let result = self
            .service
            .setup_profile_with_seed(principal, name, values, select, seed)?;
        self.changed();
        Ok(result)
    }

    pub fn profiles(&self) -> Result<Vec<simile_service::ServiceProfileSummary>> {
        let result = self.service.profiles()?;
        // Stack queries are journaled, so the resulting snapshot has changed.
        self.changed();
        Ok(result)
    }

    pub fn step_work(&mut self, limit: usize) -> Result<Vec<WorkItem>> {
        let leases = self.lease_ready_work(limit)?;
        let mut items = Vec::new();
        for lease in leases {
            self.next_handle = self.next_handle.checked_add(1).ok_or(Error::WorkHandle)?;
            let handle = format!(
                "{:032x}:{}:{}",
                self.instance, self.generation, self.next_handle
            );
            let kind = match lease.work() {
                simile_service::DeferredWork::Async { .. } => "async",
                simile_service::DeferredWork::Webhook { .. } => "webhook",
            };
            items.push(WorkItem {
                handle: handle.clone(),
                id: lease.id().to_owned(),
                name: lease.work().name().to_owned(),
                kind,
                attempt: lease.attempt(),
                work_json: serde_json::to_string(lease.work())
                    .map_err(|error| Error::Workspace(error.to_string()))?,
            });
            self.handles.insert(handle, lease);
        }
        Ok(items)
    }

    fn lease(&self, handle: &str) -> Result<&WorkLease> {
        let lease = self.handles.get(handle).ok_or(Error::WorkHandle)?;
        self.service
            .validate_work_lease(lease)
            .map_err(work_error)?;
        Ok(lease)
    }

    pub fn prepare_webhook(
        &self,
        handle: &str,
        timestamp: u64,
    ) -> Result<simile_http::PreparedWebhook> {
        let lease = self.lease(handle)?;
        let simile_service::DeferredWork::Webhook {
            method,
            content_type,
            target,
            body,
            profile,
            ..
        } = lease.work()
        else {
            return Err(Error::Workspace("work handle is not a webhook".to_owned()));
        };
        let signing = self.service.webhook_signing_for(profile.as_deref())?;
        simile_http::prepare_webhook(
            signing.as_ref(),
            method,
            content_type.as_deref(),
            target,
            body,
            timestamp,
        )
        .map_err(|error| Error::Workspace(error.to_string()))
    }

    pub fn execute_work(&mut self, handle: &str) -> Result<()> {
        let lease = self.lease(handle)?;
        if !matches!(lease.work(), simile_service::DeferredWork::Async { .. }) {
            return Err(Error::Workspace(
                "webhook delivery requires an explicit simulated acknowledgement".to_owned(),
            ));
        }
        self.service
            .execute_and_acknowledge_work(lease)
            .map_err(work_error)?;
        self.handles.remove(handle);
        self.changed();
        Ok(())
    }

    pub fn acknowledge_work(&mut self, handle: &str) -> Result<()> {
        self.service
            .acknowledge_work(self.lease(handle)?)
            .map_err(work_error)?;
        self.handles.remove(handle);
        self.changed();
        Ok(())
    }

    pub fn retry_work(&mut self, handle: &str, delay: u64) -> Result<bool> {
        let retrying = self
            .service
            .retry_work(self.lease(handle)?, delay)
            .map_err(work_error)?;
        self.handles.remove(handle);
        self.changed();
        Ok(retrying)
    }

    pub fn redeliver_dead_letter(&self) -> Result<bool> {
        let letter = self.service.dead_letters(1)?.pop();
        self.changed();
        if let Some(letter) = letter {
            self.service.redeliver(&letter)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn work_error(error: simile_service::Error) -> Error {
    match error {
        simile_service::Error::StaleWorkLease => Error::WorkHandle,
        error => Error::Service(error),
    }
}
