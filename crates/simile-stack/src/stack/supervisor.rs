use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{AdvanceReport, StackBuilder, StackSnapshot};
use crate::cache::CacheEngine;
use crate::db::DbEngine;
use crate::event_bus::BusEngine;
use crate::http::HttpEngine;
use crate::mail::MailEngine;
use crate::object_store::ObjectEngine;
use crate::queue::QueueEngine;
use crate::scheduler::SchedulerEngine;
use crate::{
    Clock, ClockMode, ClockSnapshot, CommandContext, CommandEnvelope, CommandId, CommandMetadata,
    CommandReceipt, ErrorCode, Journal, JournalEntry, JournalOutcome, LogicalTime,
    PROTOCOL_VERSION, ResourceAddress, ResourceError, Result, StackCommand, StackResult,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ReplayRecord {
    pub(crate) resource: ResourceAddress,
    pub(crate) key: String,
    pub(crate) command: StackCommand,
    pub(crate) receipt: CommandReceipt,
}

#[derive(Clone, Debug)]
pub struct Stack {
    clock: Clock,
    initial_clock: ClockSnapshot,
    resources: BTreeSet<ResourceAddress>,
    db: DbEngine,
    cache: CacheEngine,
    queue: QueueEngine,
    mail: MailEngine,
    object_store: ObjectEngine,
    event_bus: BusEngine,
    scheduler: SchedulerEngine,
    http: HttpEngine,
    journal: Journal,
    next_command: u64,
    next_event: u64,
    replays: Vec<ReplayRecord>,
}

impl Stack {
    #[must_use]
    pub fn builder(start: LogicalTime) -> StackBuilder {
        StackBuilder::manual(start)
    }

    #[must_use]
    pub fn realtime(start: LogicalTime) -> StackBuilder {
        StackBuilder::realtime(start)
    }

    pub(super) fn new(clock: Clock, resources: BTreeSet<ResourceAddress>) -> Result<Self> {
        let initial_clock = clock
            .snapshot()
            .map_err(|error| ResourceError::new(ErrorCode::Unavailable, error.to_string()))?;
        Ok(Self {
            clock,
            initial_clock,
            resources,
            db: DbEngine::default(),
            cache: CacheEngine::default(),
            queue: QueueEngine::default(),
            mail: MailEngine::default(),
            object_store: ObjectEngine::default(),
            event_bus: BusEngine::default(),
            scheduler: SchedulerEngine::default(),
            http: HttpEngine::default(),
            journal: Journal::default(),
            next_command: 0,
            next_event: 0,
            replays: Vec::new(),
        })
    }

    #[must_use]
    pub fn resources(&self) -> &BTreeSet<ResourceAddress> {
        &self.resources
    }

    /// Ensure a supervisor-owned resource exists.
    ///
    /// Service orchestration uses this narrow migration seam for internal
    /// resources introduced after a durable Stack was first created. Public
    /// service backends still come from the compiled artifact.
    pub fn ensure_resource(&mut self, resource: ResourceAddress) {
        self.resources.insert(resource);
    }

    #[must_use]
    pub const fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Inspect a queue receipt without executing a command or changing the journal.
    pub fn queue_lease_is_current(
        &self,
        resource: &ResourceAddress,
        message_id: &str,
        receipt_handle: &str,
    ) -> Result<bool> {
        if !self.resources.contains(resource) {
            return Err(ResourceError::new(
                ErrorCode::UndeclaredResource,
                format!("resource `{resource}` was not declared"),
            ));
        }
        if resource.kind != simile_spec::ResourceKind::Queue {
            return Err(ResourceError::new(
                ErrorCode::KindMismatch,
                "lease validation requires a queue",
            ));
        }
        let now = self
            .clock
            .now()
            .map_err(|error| ResourceError::new(ErrorCode::Unavailable, error.to_string()))?;
        Ok(self
            .queue
            .lease_is_current(&resource.name, message_id, receipt_handle, now))
    }

    pub fn execute(
        &mut self,
        resource: ResourceAddress,
        metadata: CommandMetadata,
        command: StackCommand,
    ) -> Result<CommandReceipt> {
        self.next_command = self.next_command.saturating_add(1);
        let envelope = CommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            resource,
            context: CommandContext {
                command_id: CommandId(self.next_command),
                at: self.clock.now().map_err(|error| {
                    ResourceError::new(ErrorCode::Unavailable, error.to_string())
                })?,
                metadata,
            },
            command,
        };
        self.execute_envelope(envelope)
    }

    pub fn execute_envelope(&mut self, envelope: CommandEnvelope) -> Result<CommandReceipt> {
        self.validate_envelope_shape(&envelope)?;
        if let Some(receipt) = self.replay(&envelope)? {
            return Ok(receipt);
        }
        self.validate_timestamp(&envelope)?;

        self.next_command = self.next_command.max(envelope.context.command_id.0);
        let result = self.route(
            &envelope.resource,
            &envelope.context,
            envelope.command.clone(),
        );
        self.record(&envelope, &result);

        let receipt = CommandReceipt {
            command_id: envelope.context.command_id,
            at: envelope.context.at,
            replayed: false,
            result: result?,
        };
        if let Some(key) = envelope.context.metadata.idempotency_key {
            self.replays.push(ReplayRecord {
                resource: envelope.resource,
                key,
                command: envelope.command,
                receipt: receipt.clone(),
            });
        }
        Ok(receipt)
    }

    pub fn advance(&mut self, millis: u64) -> Result<AdvanceReport> {
        let now = self
            .clock
            .advance(millis)
            .map_err(|error| ResourceError::new(ErrorCode::InvalidCommand, error.to_string()))?;
        Ok(AdvanceReport {
            now,
            expired_cache_entries: self.cache.purge_expired(now),
            fired_tasks: self.scheduler.run_all_due(now, 1_000),
        })
    }

    pub fn reset(&mut self) {
        self.clock = Clock::from_snapshot(self.initial_clock);
        self.db = DbEngine::default();
        self.cache = CacheEngine::default();
        self.queue = QueueEngine::default();
        self.mail = MailEngine::default();
        self.object_store = ObjectEngine::default();
        self.event_bus = BusEngine::default();
        self.scheduler = SchedulerEngine::default();
        self.http = HttpEngine::default();
        self.journal.clear();
        self.next_command = 0;
        self.next_event = 0;
        self.replays.clear();
    }

    pub fn snapshot(&self) -> Result<StackSnapshot> {
        Ok(StackSnapshot {
            clock: self
                .clock
                .snapshot()
                .map_err(|error| ResourceError::new(ErrorCode::Unavailable, error.to_string()))?,
            initial_clock: self.initial_clock,
            resources: self.resources.clone(),
            db: self.db.clone(),
            cache: self.cache.clone(),
            queue: self.queue.clone(),
            mail: self.mail.clone(),
            object_store: self.object_store.clone(),
            event_bus: self.event_bus.clone(),
            scheduler: self.scheduler.clone(),
            http: self.http.clone(),
            journal: self.journal.clone(),
            next_command: self.next_command,
            next_event: self.next_event,
            replays: self.replays.clone(),
        })
    }

    #[must_use]
    pub fn from_snapshot(snapshot: StackSnapshot) -> Self {
        Self {
            clock: Clock::from_snapshot(snapshot.clock),
            initial_clock: snapshot.initial_clock,
            resources: snapshot.resources,
            db: snapshot.db,
            cache: snapshot.cache,
            queue: snapshot.queue,
            mail: snapshot.mail,
            object_store: snapshot.object_store,
            event_bus: snapshot.event_bus,
            scheduler: snapshot.scheduler,
            http: snapshot.http,
            journal: snapshot.journal,
            next_command: snapshot.next_command,
            next_event: snapshot.next_event,
            replays: snapshot.replays,
        }
    }

    fn replay(&self, envelope: &CommandEnvelope) -> Result<Option<CommandReceipt>> {
        let Some(key) = envelope.context.metadata.idempotency_key.as_deref() else {
            return Ok(None);
        };
        let Some(record) = self
            .replays
            .iter()
            .find(|record| record.resource == envelope.resource && record.key == key)
        else {
            return Ok(None);
        };
        if record.command != envelope.command {
            return Err(ResourceError::new(
                ErrorCode::Conflict,
                "the idempotency key was already used for a different command",
            ));
        }
        let mut receipt = record.receipt.clone();
        receipt.replayed = true;
        Ok(Some(receipt))
    }

    fn record(&mut self, envelope: &CommandEnvelope, result: &Result<StackResult>) {
        self.next_event = self.next_event.saturating_add(1);
        self.journal.push(JournalEntry {
            sequence: self.next_event,
            command_id: envelope.context.command_id,
            at: envelope.context.at,
            resource: envelope.resource.clone(),
            action: envelope.command.action().to_owned(),
            operation_id: envelope.context.metadata.operation_id.clone(),
            causation_id: envelope.context.metadata.causation_id.clone(),
            correlation_id: envelope.context.metadata.correlation_id.clone(),
            outcome: match result {
                Ok(_) => JournalOutcome::Succeeded,
                Err(error) => JournalOutcome::Failed { code: error.code },
            },
        });
    }

    fn validate_envelope_shape(&self, envelope: &CommandEnvelope) -> Result<()> {
        if envelope.protocol_version != PROTOCOL_VERSION {
            return Err(ResourceError::new(
                ErrorCode::InvalidCommand,
                format!(
                    "stack protocol version {} is unsupported; expected {PROTOCOL_VERSION}",
                    envelope.protocol_version
                ),
            ));
        }
        if !self.resources.contains(&envelope.resource) {
            return Err(ResourceError::new(
                ErrorCode::UndeclaredResource,
                format!("resource `{}` was not declared", envelope.resource),
            ));
        }
        if envelope.resource.kind != envelope.command.kind() {
            return Err(ResourceError::new(
                ErrorCode::KindMismatch,
                format!(
                    "resource `{}` cannot execute a `{}` command",
                    envelope.resource,
                    envelope.command.kind()
                ),
            ));
        }
        Ok(())
    }

    fn validate_timestamp(&self, envelope: &CommandEnvelope) -> Result<()> {
        if self.clock.mode() == ClockMode::Manual
            && self
                .clock
                .now()
                .map_err(|error| ResourceError::new(ErrorCode::Unavailable, error.to_string()))?
                != envelope.context.at
        {
            return Err(ResourceError::new(
                ErrorCode::InvalidCommand,
                "the command timestamp does not match the component's logical clock",
            ));
        }
        Ok(())
    }

    fn route(
        &mut self,
        resource: &ResourceAddress,
        context: &CommandContext,
        command: StackCommand,
    ) -> Result<StackResult> {
        match command {
            StackCommand::Db(command) => self
                .db
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Db),
            StackCommand::Cache(command) => self
                .cache
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Cache),
            StackCommand::Queue(command) => self
                .queue
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Queue),
            StackCommand::Mail(command) => self
                .mail
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Mail),
            StackCommand::ObjectStore(command) => self
                .object_store
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::ObjectStore),
            StackCommand::EventBus(command) => self
                .event_bus
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::EventBus),
            StackCommand::Scheduler(command) => self
                .scheduler
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Scheduler),
            StackCommand::Http(command) => self
                .http
                .execute(&resource.name, context, command)
                .map(Box::new)
                .map(StackResult::Http),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use simile_spec::ResourceKind;

    use super::Stack;
    use crate::{
        CacheCommand, CacheResult, CommandMetadata, LogicalTime, ResourceAddress, StackCommand,
        StackResult,
    };

    #[test]
    fn idempotency_replays_the_original_result() -> crate::Result<()> {
        let mut stack = Stack::builder(LogicalTime::UNIX_EPOCH)
            .resource(ResourceKind::Cache, "sessions")?
            .build()?;
        let resource = ResourceAddress::new(ResourceKind::Cache, "sessions")?;
        let metadata = CommandMetadata {
            idempotency_key: Some("one".to_owned()),
            ..CommandMetadata::default()
        };
        let command = StackCommand::Cache(CacheCommand::Set {
            key: "token".to_owned(),
            value: json!("value"),
            ttl_ms: None,
        });
        let first = stack.execute(resource.clone(), metadata.clone(), command.clone());
        stack.advance(10)?;
        let second = stack.execute(resource, metadata, command);
        assert!(matches!(
            first,
            Ok(crate::CommandReceipt {
                replayed: false,
                result: StackResult::Cache(result),
                ..
            }) if matches!(*result, CacheResult::Stored(_))
        ));
        assert!(matches!(second, Ok(receipt) if receipt.replayed));
        assert_eq!(stack.journal().entries().len(), 1);
        Ok(())
    }
}
