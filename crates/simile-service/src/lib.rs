#![forbid(unsafe_code)]

//! Protocol-neutral composition of operation execution and Simile Stack.
//!
//! The runtime proves and commits the public operation result. The supervisor
//! then applies synchronous consequences transactionally, checkpoints the
//! complete Stack, and exposes asynchronous work for post-response dispatch.

mod profile;
mod profile_name;
mod state;
mod work;

use std::collections::BTreeMap;
#[cfg(feature = "persistence")]
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use web_time::{SystemTime, UNIX_EPOCH};

use simile_artifact::Artifact;
use simile_runtime::{
    AuthenticatedProfile, ExecutionOptions, IncomingParameters, OperationCall, OperationExecution,
    OperationValueCall, Runtime, authenticate_with_profiles,
};
use simile_spec::ResourceKind;
use simile_stack::{
    CommandMetadata, DbCommand, JournalOutcome, LogicalTime, MemoryStateStore, QueueCommand,
    QueueResult, ResourceAddress, ResourceError, Stack, StackCommand, StackResult, StackSnapshot,
    StateStore, StateStoreError, WriteCondition,
};
#[cfg(feature = "persistence")]
use simile_stack::{RedbStateStore, StateIdentity};
use thiserror::Error;

pub use profile::{
    ProfileFieldSummary, ProfileValue, ServiceProfile, ServiceProfileSummary, SetupProfileResult,
    validate_profile_name,
};
pub use simile_spec::ProfileStatus;
use work::{DEAD_LETTERS, DEFAULT_VISIBILITY_TIMEOUT_MS, OUTBOX, WORK_STATE};
pub use work::{DeadLetter, DeferredCommand, DeferredWork, WorkLease};

pub type Result<T> = std::result::Result<T, Error>;

/// Whether every hard prerequisite for one deferred item has completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyReadiness {
    Ready,
    Waiting,
    Blocked,
}

/// One running service: public operations, declared backends, and durable state.
pub struct Service {
    runtime: Runtime,
    stack: Mutex<Stack>,
    store: Arc<dyn StateStore>,
}

/// Authentication result enriched with the durable profile that supplied it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceAuthentication {
    principal: Option<String>,
    profile: Option<String>,
}

impl ServiceAuthentication {
    #[must_use]
    pub fn principal(&self) -> Option<&str> {
        self.principal.as_deref()
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

impl Service {
    /// Build an isolated service with in-memory state.
    pub fn memory(artifact: Artifact) -> Result<Self> {
        Self::with_store(artifact, Arc::new(MemoryStateStore::default()))
    }

    /// Build an isolated service with an explicit clock, including a manual clock
    /// that never reads the host's wall or monotonic time.
    pub fn memory_with_clock(
        artifact: Artifact,
        clock: simile_stack::ClockSnapshot,
    ) -> Result<Self> {
        Self::with_store_and_clock(artifact, Arc::new(MemoryStateStore::default()), clock)
    }

    /// Build a service whose complete Stack survives process restarts.
    #[cfg(feature = "persistence")]
    pub fn durable(artifact: Artifact, path: impl AsRef<Path>) -> Result<Self> {
        let summary = artifact.summary();
        let identity = StateIdentity::new(summary.service, summary.payload_sha256);
        let store = RedbStateStore::open_with_identity(path, identity)?;
        Self::with_store(artifact, Arc::new(store))
    }

    /// Build a service over an injected state store.
    ///
    /// A durable caller normally supplies `RedbStateStore`; tests and one-shot
    /// tools can use `MemoryStateStore` without changing execution semantics.
    pub fn with_store(artifact: Artifact, store: Arc<dyn StateStore>) -> Result<Self> {
        Self::with_store_and_clock(
            artifact,
            store,
            simile_stack::ClockSnapshot {
                mode: simile_stack::ClockMode::Realtime,
                now: now(),
            },
        )
    }

    /// Restore an existing store or initialize it using the supplied clock.
    pub fn with_store_and_clock(
        artifact: Artifact,
        store: Arc<dyn StateStore>,
        clock: simile_stack::ClockSnapshot,
    ) -> Result<Self> {
        let mut resources = artifact
            .resources()
            .map(|resource| ResourceAddress::new(resource.kind(), resource.name()))
            .collect::<simile_stack::Result<Vec<_>>>()?;
        resources.push(ResourceAddress::new(ResourceKind::Queue, OUTBOX)?);
        resources.push(ResourceAddress::new(ResourceKind::Queue, DEAD_LETTERS)?);
        resources.push(ResourceAddress::new(ResourceKind::Db, WORK_STATE)?);
        resources.push(ResourceAddress::new(
            ResourceKind::Db,
            profile::PROFILE_STATE,
        )?);
        let builder = match clock.mode {
            simile_stack::ClockMode::Manual => simile_stack::StackBuilder::manual(clock.now),
            simile_stack::ClockMode::Realtime => Stack::realtime(clock.now),
        };
        let initial = builder.resources(resources)?.build()?;
        let stack = match store.load()? {
            Some(snapshot) => {
                let mut restored = Stack::from_snapshot(snapshot);
                restored.ensure_resource(ResourceAddress::new(ResourceKind::Queue, OUTBOX)?);
                restored.ensure_resource(ResourceAddress::new(ResourceKind::Queue, DEAD_LETTERS)?);
                restored.ensure_resource(ResourceAddress::new(ResourceKind::Db, WORK_STATE)?);
                restored.ensure_resource(ResourceAddress::new(
                    ResourceKind::Db,
                    profile::PROFILE_STATE,
                )?);
                if restored.resources() != initial.resources() {
                    return Err(Error::ResourceSetChanged);
                }
                restored
            }
            None => {
                store.save(&initial.snapshot()?)?;
                initial
            }
        };
        Ok(Self {
            runtime: Runtime::from_artifact(artifact),
            stack: Mutex::new(stack),
            store,
        })
    }

    pub fn execute(&self, call: OperationCall<'_>) -> Result<OperationExecution> {
        self.execute_with_options(call, ExecutionOptions::default())
    }

    pub fn execute_with_options(
        &self,
        call: OperationCall<'_>,
        options: ExecutionOptions<'_>,
    ) -> Result<OperationExecution> {
        self.execute_for_profile_with_options(call, options, None)
    }

    /// Authenticate against static fixtures and every active service profile.
    pub fn authenticate(
        &self,
        operation: &str,
        incoming: &IncomingParameters,
    ) -> Result<ServiceAuthentication> {
        let profiles = self
            .profile_records()?
            .iter()
            .map(ServiceProfile::credential_profile)
            .collect::<Vec<_>>();
        let authenticated =
            authenticate_with_profiles(self.artifact(), operation, incoming, &profiles)?;
        Ok(service_authentication(authenticated))
    }

    /// Execute one call in the tenant context established by authentication.
    pub fn execute_for_profile_with_options(
        &self,
        call: OperationCall<'_>,
        options: ExecutionOptions<'_>,
        profile: Option<&str>,
    ) -> Result<OperationExecution> {
        let profile = profile
            .map(|name| self.profile(name))
            .transpose()?
            .flatten();
        // Why: POST-based public lookup/page routes carry their selector in JSON rather than in
        // path parameters, so state resolution needs the request value before runtime synthesis.
        let request_body = call
            .content_type
            .filter(|content_type| {
                *content_type == "application/json" || content_type.ends_with("+json")
            })
            .and_then(|_| serde_json::from_slice(call.body).ok());
        let response_request = state::response_request_body(
            self,
            call.operation,
            request_body.as_ref(),
            profile.as_ref().map(|profile| profile.id.as_str()),
        )?;
        let state = state::resolve(
            self,
            call.operation,
            options.parameters,
            request_body.as_ref(),
            profile.as_ref().map(|profile| profile.id.as_str()),
        )?;
        let mutation = if matches!(state, state::StateResolution::None) {
            state::mutation_baseline(
                self,
                call.operation,
                request_body.as_ref(),
                profile.as_ref().map(|profile| profile.id.as_str()),
            )?
        } else {
            state::StateResolution::None
        };
        let response_body = match (&state, &mutation) {
            (state::StateResolution::Found(value), _)
            | (_, state::StateResolution::Found(value)) => Some(value),
            (state::StateResolution::None | state::StateResolution::Missing, _) => {
                options.response_body
            }
        };
        let response_status = match state {
            state::StateResolution::Missing
                if self
                    .artifact()
                    .operation(call.operation)
                    .is_some_and(state::declares_not_found) =>
            {
                Some(404)
            }
            _ => options.response_status,
        };
        let execution = self.runtime.execute_with_options(
            call,
            ExecutionOptions {
                outcome: response_request.outcome.as_deref().or(options.outcome),
                response_body,
                response_status,
                response_request_body: response_request
                    .body
                    .as_ref()
                    .or(options.response_request_body),
                ..options
            },
        )?;
        self.commit(&execution, options.parameters, profile.as_ref())?;
        Ok(execution)
    }

    pub fn execute_value(&self, call: OperationValueCall<'_>) -> Result<OperationExecution> {
        self.execute_value_with_options(call, ExecutionOptions::default())
    }

    pub fn execute_value_with_options(
        &self,
        call: OperationValueCall<'_>,
        options: ExecutionOptions<'_>,
    ) -> Result<OperationExecution> {
        let response_request = state::response_request_body(self, call.operation, call.body, None)?;
        let state = state::resolve(self, call.operation, options.parameters, call.body, None)?;
        let mutation = if matches!(state, state::StateResolution::None) {
            state::mutation_baseline(self, call.operation, call.body, None)?
        } else {
            state::StateResolution::None
        };
        let response_body = match (&state, &mutation) {
            (state::StateResolution::Found(value), _)
            | (_, state::StateResolution::Found(value)) => Some(value),
            (state::StateResolution::None | state::StateResolution::Missing, _) => {
                options.response_body
            }
        };
        let response_status = match state {
            state::StateResolution::Missing
                if self
                    .artifact()
                    .operation(call.operation)
                    .is_some_and(state::declares_not_found) =>
            {
                Some(404)
            }
            _ => options.response_status,
        };
        let execution = self.runtime.execute_value_with_options(
            call,
            ExecutionOptions {
                outcome: response_request.outcome.as_deref().or(options.outcome),
                response_body,
                response_status,
                response_request_body: response_request
                    .body
                    .as_ref()
                    .or(options.response_request_body),
                ..options
            },
        )?;
        self.commit(&execution, options.parameters, None)?;
        Ok(execution)
    }

    /// Lease available deferred work. Unacknowledged items become visible again
    /// after `visibility_timeout_milliseconds`.
    pub fn lease_work(
        &self,
        limit: usize,
        visibility_timeout_milliseconds: Option<u64>,
    ) -> Result<Vec<WorkLease>> {
        let receipt = self.execute_stack(
            ResourceAddress::new(ResourceKind::Queue, OUTBOX)?,
            CommandMetadata::default(),
            StackCommand::Queue(QueueCommand::Receive {
                max_messages: limit,
                visibility_timeout_ms: visibility_timeout_milliseconds
                    .unwrap_or(DEFAULT_VISIBILITY_TIMEOUT_MS),
            }),
        )?;
        match receipt.result {
            StackResult::Queue(result) => match *result {
                QueueResult::Messages(messages) => {
                    messages.into_iter().map(work::decode_lease).collect()
                }
                _ => Err(Error::InvalidWork(
                    "outbox receive returned an unexpected result".to_owned(),
                )),
            },
            _ => Err(Error::InvalidWork(
                "outbox receive returned a non-queue result".to_owned(),
            )),
        }
    }

    /// Execute the typed commands carried by one asynchronous work item.
    pub fn execute_work(&self, lease: &WorkLease) -> Result<()> {
        self.execute_leased_work(lease, false)
    }

    /// Execute and acknowledge work in one transaction, rolling back effects if
    /// its receipt expires or the resulting checkpoint cannot be saved.
    pub fn execute_and_acknowledge_work(&self, lease: &WorkLease) -> Result<()> {
        self.execute_leased_work(lease, true)
    }

    /// Check a receipt without changing resource state, counters, or the journal.
    pub fn validate_work_lease(&self, lease: &WorkLease) -> Result<()> {
        let stack = lock(&self.stack)?;
        validate_lease(&stack, OUTBOX, lease)
    }

    fn execute_leased_work(&self, lease: &WorkLease, acknowledge: bool) -> Result<()> {
        let mut stack = lock(&self.stack)?;
        validate_lease(&stack, OUTBOX, lease)?;
        if !acknowledge && !matches!(lease.work(), DeferredWork::Async { .. }) {
            return Ok(());
        }
        let before = stack.snapshot()?;
        let result = (|| {
            if let DeferredWork::Async {
                operation,
                commands,
                ..
            } = lease.work()
            {
                for command in commands {
                    stack.execute(
                        command.resource.parse::<ResourceAddress>()?,
                        CommandMetadata {
                            operation_id: Some(operation.clone()),
                            causation_id: Some(lease.id().to_owned()),
                            ..CommandMetadata::default()
                        },
                        command.command.clone(),
                    )?;
                }
            }
            if acknowledge {
                record_webhook_outcome(&mut stack, lease, true)?;
                acknowledge_lease(&mut stack, OUTBOX, lease)?;
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)
    }

    /// Remove successfully completed work from the durable outbox.
    pub fn acknowledge_work(&self, lease: &WorkLease) -> Result<()> {
        self.finish_lease(lease, LeaseDisposition::Acknowledge)
    }

    /// Release failed work with a retry delay, or move it to dead letters once
    /// its declared attempt budget is exhausted.
    pub fn retry_work(&self, lease: &WorkLease, delay_milliseconds: u64) -> Result<bool> {
        let exhausted = lease.attempt() >= lease.work().max_attempts();
        self.finish_lease(
            lease,
            if exhausted {
                LeaseDisposition::DeadLetter
            } else {
                LeaseDisposition::Retry(delay_milliseconds)
            },
        )?;
        Ok(!exhausted)
    }

    /// Return leased work to the outbox without consuming a delivery attempt.
    ///
    /// The dispatcher uses this when a hard causal prerequisite is still in
    /// flight. Queue receive counts remain transport diagnostics; the retry
    /// budget tracks actual execution failures inside the work envelope.
    pub fn defer_work(&self, lease: &WorkLease, delay_milliseconds: u64) -> Result<()> {
        let mut stack = lock(&self.stack)?;
        validate_lease(&stack, OUTBOX, lease)?;
        let before = stack.snapshot()?;
        let result = stack
            .execute(
                ResourceAddress::new(ResourceKind::Queue, OUTBOX)?,
                CommandMetadata::default(),
                StackCommand::Queue(QueueCommand::Nack {
                    receipt_handle: lease.receipt_handle.clone(),
                    delay_ms: delay_milliseconds,
                }),
            )
            .map_err(Error::from)
            .and_then(|receipt| match receipt.result {
                StackResult::Queue(result) if matches!(*result, QueueResult::Released(true)) => {
                    Ok(())
                }
                _ => Err(Error::StaleWorkLease),
            });
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)
    }

    /// Move one item directly to dead letters because a hard prerequisite can
    /// no longer complete.
    pub fn dead_letter_work(&self, lease: &WorkLease) -> Result<()> {
        self.finish_lease(lease, LeaseDisposition::DeadLetter)
    }

    /// Inspect durable causal markers for a webhook's hard prerequisites.
    pub fn dependency_readiness(&self, lease: &WorkLease) -> Result<DependencyReadiness> {
        let DeferredWork::Webhook {
            correlation_id,
            after,
            ..
        } = lease.work()
        else {
            return Ok(DependencyReadiness::Ready);
        };
        if after.is_empty() {
            return Ok(DependencyReadiness::Ready);
        }
        let stack = lock(&self.stack)?;
        let resource = ResourceAddress::new(ResourceKind::Db, WORK_STATE)?;
        let mut readiness = DependencyReadiness::Ready;
        for dependency in after {
            let outcome = stack.journal().entries().iter().rev().find(|entry| {
                entry.resource == resource
                    && entry.correlation_id.as_deref() == Some(correlation_id.as_str())
                    && entry.causation_id.as_deref() == Some(dependency.as_str())
                    && matches!(entry.outcome, JournalOutcome::Succeeded)
            });
            match outcome.map(|entry| entry.action.as_str()) {
                Some("put") => {}
                Some("delete") => return Ok(DependencyReadiness::Blocked),
                _ => readiness = DependencyReadiness::Waiting,
            }
        }
        Ok(readiness)
    }

    /// Lease exhausted work for operator inspection. Items remain available
    /// after the visibility timeout unless acknowledged through `redeliver`.
    pub fn dead_letters(&self, limit: usize) -> Result<Vec<DeadLetter>> {
        let receipt = self.execute_stack(
            ResourceAddress::new(ResourceKind::Queue, DEAD_LETTERS)?,
            CommandMetadata::default(),
            StackCommand::Queue(QueueCommand::Receive {
                max_messages: limit,
                visibility_timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
            }),
        )?;
        match receipt.result {
            StackResult::Queue(result) => match *result {
                QueueResult::Messages(messages) => {
                    messages.into_iter().map(work::decode_dead_letter).collect()
                }
                _ => Err(Error::InvalidWork(
                    "dead-letter receive returned an unexpected result".to_owned(),
                )),
            },
            _ => Err(Error::InvalidWork(
                "dead-letter receive returned a non-queue result".to_owned(),
            )),
        }
    }

    /// Move one exhausted item back to the durable outbox with a fresh retry
    /// budget, then acknowledge its dead-letter record atomically.
    pub fn redeliver(&self, dead_letter: &DeadLetter) -> Result<()> {
        let mut stack = lock(&self.stack)?;
        validate_lease(&stack, DEAD_LETTERS, &dead_letter.lease)?;
        let before = stack.snapshot()?;
        let result = (|| {
            stack.execute(
                ResourceAddress::new(ResourceKind::Queue, OUTBOX)?,
                CommandMetadata {
                    causation_id: Some(dead_letter.id().to_owned()),
                    operation_id: Some(dead_letter.work().operation().to_owned()),
                    ..CommandMetadata::default()
                },
                StackCommand::Queue(QueueCommand::Publish {
                    body: work::encode(&dead_letter.work().with_fresh_retry_budget())?,
                    attributes: work::attributes(dead_letter.work()),
                    delay_ms: 0,
                }),
            )?;
            acknowledge_lease(&mut stack, DEAD_LETTERS, &dead_letter.lease)?;
            Ok::<(), Error>(())
        })();
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)
    }

    #[must_use]
    pub const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Return the compiled contract that defines this service.
    #[must_use]
    pub fn artifact(&self) -> &Artifact {
        self.runtime.artifact()
    }

    pub fn snapshot(&self) -> Result<StackSnapshot> {
        Ok(lock(&self.stack)?.snapshot()?)
    }

    /// Advance a manual service clock and persist the resulting resource changes.
    pub fn advance(&self, milliseconds: u64) -> Result<simile_stack::AdvanceReport> {
        let mut stack = lock(&self.stack)?;
        let before = stack.snapshot()?;
        let report = stack.advance(milliseconds)?;
        self.checkpoint(&mut stack, before)?;
        Ok(report)
    }

    /// Read the service timeline without mutating state or leasing work.
    pub fn clock(&self) -> Result<simile_stack::ClockSnapshot> {
        Ok(self.snapshot()?.clock())
    }

    fn commit(
        &self,
        execution: &OperationExecution,
        parameters: Option<&simile_runtime::ValidatedParameters>,
        profile: Option<&ServiceProfile>,
    ) -> Result<()> {
        let mut stack = lock(&self.stack)?;
        let before = stack.snapshot()?;
        let result = self.apply_commit(&mut stack, execution, parameters, profile);
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)
    }

    fn apply_commit(
        &self,
        stack: &mut Stack,
        execution: &OperationExecution,
        parameters: Option<&simile_runtime::ValidatedParameters>,
        profile: Option<&ServiceProfile>,
    ) -> Result<()> {
        for action in execution.effects().synchronous() {
            let resource = action.resource().as_str().parse::<ResourceAddress>()?;
            let command = StackCommand::from_effect(action.kind(), action.arguments())?;
            // Why: provider-authored effects describe public resources, while a configured
            // profile is an independent local consumer environment whose state must not leak to
            // another credential using the same emulated service.
            let command = state::scope_command(command, profile.map(|profile| profile.id.as_str()));
            stack.execute(
                resource,
                CommandMetadata {
                    operation_id: Some(execution.operation().to_owned()),
                    ..CommandMetadata::default()
                },
                command,
            )?;
        }
        let correlation_id = format!(
            "work-{}",
            stack
                .journal()
                .entries()
                .last()
                .map_or(1, |entry| entry.sequence.saturating_add(1))
        );
        for envelope in work::planned(
            execution,
            &correlation_id,
            profile.map(|profile| profile.name.as_str()),
            profile.and_then(|profile| profile.webhook_target.as_deref()),
        )? {
            let command = QueueCommand::Publish {
                body: work::encode(&envelope.work)?,
                attributes: work::attributes(&envelope.work),
                delay_ms: envelope.delay_milliseconds,
            };
            stack.execute(
                ResourceAddress::new(ResourceKind::Queue, OUTBOX)?,
                CommandMetadata {
                    operation_id: Some(execution.operation().to_owned()),
                    correlation_id: Some(correlation_id.clone()),
                    ..CommandMetadata::default()
                },
                StackCommand::Queue(command),
            )?;
        }
        if let Some((resource, command)) = state::delete_command(
            self.artifact(),
            execution.operation(),
            parameters,
            profile.map(|profile| profile.id.as_str()),
        ) {
            stack.execute(
                resource,
                CommandMetadata {
                    operation_id: Some(execution.operation().to_owned()),
                    ..CommandMetadata::default()
                },
                command,
            )?;
        }
        if let Some(profile) = profile {
            profile::put_profile(stack, profile)?;
        }
        Ok(())
    }

    fn execute_stack(
        &self,
        resource: ResourceAddress,
        metadata: CommandMetadata,
        command: StackCommand,
    ) -> Result<simile_stack::CommandReceipt> {
        let mut stack = lock(&self.stack)?;
        let before = stack.snapshot()?;
        let receipt = match stack.execute(resource, metadata, command) {
            Ok(receipt) => receipt,
            Err(error) => {
                *stack = Stack::from_snapshot(before);
                return Err(error.into());
            }
        };
        self.checkpoint(&mut stack, before)?;
        Ok(receipt)
    }

    pub(crate) fn read_stack(
        &self,
        resource: ResourceAddress,
        command: StackCommand,
    ) -> Result<simile_stack::CommandReceipt> {
        self.execute_stack(resource, CommandMetadata::default(), command)
    }

    fn finish_lease(&self, lease: &WorkLease, disposition: LeaseDisposition) -> Result<()> {
        let mut stack = lock(&self.stack)?;
        validate_lease(&stack, OUTBOX, lease)?;
        let before = stack.snapshot()?;
        let result = (|| {
            if matches!(disposition, LeaseDisposition::Acknowledge) {
                record_webhook_outcome(&mut stack, lease, true)?;
            }
            if matches!(disposition, LeaseDisposition::DeadLetter) {
                stack.execute(
                    ResourceAddress::new(ResourceKind::Queue, DEAD_LETTERS)?,
                    CommandMetadata {
                        causation_id: Some(lease.id().to_owned()),
                        operation_id: Some(lease.work().operation().to_owned()),
                        ..CommandMetadata::default()
                    },
                    StackCommand::Queue(QueueCommand::Publish {
                        body: work::encode(lease.work())?,
                        attributes: BTreeMap::from([
                            ("attempts".to_owned(), lease.attempt().to_string()),
                            ("name".to_owned(), lease.work().name().to_owned()),
                        ]),
                        delay_ms: 0,
                    }),
                )?;
                record_webhook_outcome(&mut stack, lease, false)?;
            }
            if let LeaseDisposition::Retry(delay_ms) = disposition {
                let failed = lease.work().after_failure();
                stack.execute(
                    ResourceAddress::new(ResourceKind::Queue, OUTBOX)?,
                    CommandMetadata {
                        causation_id: Some(lease.id().to_owned()),
                        operation_id: Some(lease.work().operation().to_owned()),
                        ..CommandMetadata::default()
                    },
                    StackCommand::Queue(QueueCommand::Publish {
                        body: work::encode(&failed)?,
                        attributes: work::attributes(&failed),
                        delay_ms,
                    }),
                )?;
            }
            acknowledge_lease(&mut stack, OUTBOX, lease)?;
            Ok::<(), Error>(())
        })();
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)
    }

    fn checkpoint(&self, stack: &mut Stack, before: StackSnapshot) -> Result<()> {
        let after = match stack.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                *stack = Stack::from_snapshot(before.clone());
                return Err(error.into());
            }
        };
        if let Err(error) = self.store.save(&after) {
            *stack = Stack::from_snapshot(before);
            return Err(error.into());
        }
        Ok(())
    }
}

fn service_authentication(authenticated: Option<AuthenticatedProfile>) -> ServiceAuthentication {
    let profile = authenticated
        .as_ref()
        .filter(|profile| profile.is_dynamic())
        .map(|profile| profile.name().to_owned());
    let principal = authenticated.map(|profile| profile.principal().to_owned());
    ServiceAuthentication { principal, profile }
}

enum LeaseDisposition {
    Acknowledge,
    Retry(u64),
    DeadLetter,
}

fn validate_lease(stack: &Stack, queue: &str, lease: &WorkLease) -> Result<()> {
    if stack.queue_lease_is_current(
        &ResourceAddress::new(ResourceKind::Queue, queue)?,
        lease.id(),
        &lease.receipt_handle,
    )? {
        Ok(())
    } else {
        Err(Error::StaleWorkLease)
    }
}

fn acknowledge_lease(stack: &mut Stack, queue: &str, lease: &WorkLease) -> Result<()> {
    let receipt = stack.execute(
        ResourceAddress::new(ResourceKind::Queue, queue)?,
        CommandMetadata::default(),
        StackCommand::Queue(QueueCommand::Ack {
            receipt_handle: lease.receipt_handle.clone(),
        }),
    )?;
    match receipt.result {
        StackResult::Queue(result) if matches!(*result, QueueResult::Acknowledged(true)) => Ok(()),
        _ => Err(Error::StaleWorkLease),
    }
}

fn record_webhook_outcome(stack: &mut Stack, lease: &WorkLease, completed: bool) -> Result<()> {
    let DeferredWork::Webhook {
        name,
        operation,
        correlation_id,
        ..
    } = lease.work()
    else {
        return Ok(());
    };
    let command = if completed {
        DbCommand::Put {
            collection: "webhook_deliveries".to_owned(),
            key: format!("{correlation_id}:{name}"),
            value: serde_json::json!({ "completed": true }),
            condition: WriteCondition::Any,
        }
    } else {
        DbCommand::Delete {
            collection: "webhook_deliveries".to_owned(),
            key: format!("{correlation_id}:{name}"),
            condition: WriteCondition::Any,
        }
    };
    stack.execute(
        ResourceAddress::new(ResourceKind::Db, WORK_STATE)?,
        CommandMetadata {
            operation_id: Some(operation.clone()),
            causation_id: Some(name.clone()),
            correlation_id: Some(correlation_id.clone()),
            ..CommandMetadata::default()
        },
        StackCommand::Db(command),
    )?;
    Ok(())
}

fn now() -> LogicalTime {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default();
    LogicalTime::from_unix_millis(milliseconds)
}

fn lock(stack: &Mutex<Stack>) -> Result<MutexGuard<'_, Stack>> {
    stack.lock().map_err(|_| Error::StateUnavailable)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Runtime(#[from] simile_runtime::Error),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Store(#[from] StateStoreError),
    #[error("service state is unavailable")]
    StateUnavailable,
    #[error("persisted resources do not match the compiled service")]
    ResourceSetChanged,
    #[error("deferred work is invalid: {0}")]
    InvalidWork(String),
    #[error("work lease is expired or no longer current")]
    StaleWorkLease,
    #[error("state-backed response cannot be represented by the operation contract")]
    StateResponseUnavailable,
    #[error("state read returned an unexpected result for {0}")]
    InvalidStateRead(&'static str),
    #[error("state projection for operation `{operation}` failed: {reason}")]
    InvalidStateProjection { operation: String, reason: String },
    #[error(
        "this service does not declare principal setup; add a `principal_setup` block to its configuration"
    )]
    SetupNotDeclared,
    #[error("principal `{principal}` does not declare principal setup")]
    PrincipalSetupNotDeclared { principal: String },
    #[error(
        "this service declares setup for more than one principal ({principals:?}); select one explicitly"
    )]
    PrincipalSetupAmbiguous { principals: Vec<String> },
    #[error("setup operationId `{0}` is missing from the compiled artifact")]
    SetupOperationMissing(String),
    #[error("setup operation `{operation}` has no request body for its collected values")]
    SetupRequestBodyMissing { operation: String },
    #[error("setup requires value `{name}`")]
    MissingSetupValue { name: String },
    #[error("setup does not collect a value named `{name}`")]
    UnknownSetupValue { name: String },
    #[error("setup value `{name}` must be {expected}")]
    InvalidSetupValue {
        name: String,
        expected: &'static str,
    },
    #[error("setup request pointer `{pointer}` cannot be applied to the request body")]
    InvalidSetupPointer { pointer: String },
    #[error("setup response does not contain `{pointer}` required by issued value `{issue}`")]
    SetupResponseValueMissing { issue: String, pointer: String },
    #[error("setup issued credential `{issue}` is not a string")]
    SetupCredentialNotString { issue: String },
    #[error("invalid profile name `{profile}`; use lowercase letters, digits, `.`, `_`, or `-`")]
    InvalidProfileName { profile: String },
    #[error("every generated profile name is already in use; pass --profile with a unique name")]
    ProfileNameSpaceExhausted,
    #[error("profile `{profile}` does not exist")]
    ProfileNotFound { profile: String },
    #[error(
        "profile `{profile}` belongs to principal `{expected}` and cannot be reconfigured as `{actual}`"
    )]
    ProfilePrincipalMismatch {
        profile: String,
        expected: String,
        actual: String,
    },
    #[error("persisted profile state is invalid")]
    InvalidProfileState,
    #[error("persisted profile could not be encoded or decoded: {0}")]
    ProfileEncoding(String),
}
