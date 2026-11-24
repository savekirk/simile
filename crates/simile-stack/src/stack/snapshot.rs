use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use simile_spec::ResourceKind;

use super::supervisor::ReplayRecord;
use crate::JournalEntry;
use crate::cache::CacheEngine;
use crate::cache::CacheEntry;
use crate::db::DbEngine;
use crate::db::StoredDocument;
use crate::event_bus::BusEngine;
use crate::event_bus::BusState;
use crate::http::HttpEngine;
use crate::http::HttpState;
use crate::mail::MailEngine;
use crate::mail::MailMessage;
use crate::object_store::ObjectEngine;
use crate::object_store::StoredObject;
use crate::queue::QueueEngine;
use crate::queue::StoredMessage;
use crate::scheduler::SchedulerEngine;
use crate::{ClockSnapshot, Journal, LogicalTime, ResourceAddress, ScheduledTask};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceReport {
    pub now: LogicalTime,
    pub expired_cache_entries: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fired_tasks: BTreeMap<String, Vec<ScheduledTask>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackSnapshot {
    pub(super) clock: ClockSnapshot,
    pub(super) initial_clock: ClockSnapshot,
    pub(super) resources: BTreeSet<ResourceAddress>,
    pub(super) db: DbEngine,
    pub(super) cache: CacheEngine,
    pub(super) queue: QueueEngine,
    pub(super) mail: MailEngine,
    pub(super) object_store: ObjectEngine,
    pub(super) event_bus: BusEngine,
    pub(super) scheduler: SchedulerEngine,
    pub(super) http: HttpEngine,
    pub(super) journal: Journal,
    pub(super) next_command: u64,
    pub(super) next_event: u64,
    pub(super) replays: Vec<ReplayRecord>,
}

/// Storage-neutral records that let durable stores update independent pieces
/// of a Stack without coupling resource engines to a database implementation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StackRecords {
    pub(crate) resources: BTreeMap<ResourceAddress, ResourceState>,
    pub(crate) replays: BTreeMap<String, ReplayRecord>,
    pub(crate) journal: BTreeMap<u64, JournalEntry>,
    pub(crate) counters: StackCounters,
    pub(crate) clocks: StackClocks,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "state", rename_all = "snake_case")]
pub(crate) enum ResourceState {
    Db(Option<BTreeMap<String, BTreeMap<String, StoredDocument>>>),
    Cache(Option<BTreeMap<String, CacheEntry>>),
    Queue(Option<VecDeque<StoredMessage>>),
    Mail(Option<BTreeMap<String, MailMessage>>),
    ObjectStore(Option<BTreeMap<String, StoredObject>>),
    EventBus(Option<BusState>),
    Scheduler(Option<BTreeMap<String, ScheduledTask>>),
    Http(Option<HttpState>),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackCounters {
    pub(crate) next_command: u64,
    pub(crate) next_event: u64,
    pub(crate) queue_message: u64,
    pub(crate) queue_receipt: u64,
    pub(crate) mail_message: u64,
    pub(crate) scheduler_task: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackClocks {
    pub(crate) current: ClockSnapshot,
    pub(crate) initial: ClockSnapshot,
}

impl StackSnapshot {
    #[must_use]
    pub const fn clock(&self) -> ClockSnapshot {
        self.clock
    }

    #[must_use]
    pub const fn initial_clock(&self) -> ClockSnapshot {
        self.initial_clock
    }

    /// Reject undeclared engine state, duplicate journal/replay records, and
    /// counters that could reuse committed command or event identities.
    pub fn validate(&self) -> Result<(), String> {
        let rebuilt = Self::from_records(self.to_records())?;
        if rebuilt != *self {
            return Err("snapshot records are inconsistent".to_owned());
        }
        if self
            .journal
            .entries()
            .windows(2)
            .any(|pair| pair[0].sequence >= pair[1].sequence)
            || self.journal.entries().iter().any(|entry| {
                entry.sequence > self.next_event || entry.command_id.0 > self.next_command
            })
        {
            return Err("snapshot journal counters are inconsistent".to_owned());
        }
        Ok(())
    }

    /// Return a JSON view of one resource's engine state.
    ///
    /// State values are intentionally available only through this explicit
    /// inspection method. Normal journal inspection stays payload-free, while
    /// operator tooling can opt in when it needs to diagnose stored state.
    #[must_use]
    pub fn resource_state(&self, address: &ResourceAddress) -> Option<serde_json::Value> {
        if !self.resources.contains(address) {
            return None;
        }
        match address.kind {
            ResourceKind::Db => serde_json::to_value(self.db.resources.get(&address.name)).ok(),
            ResourceKind::Cache => {
                serde_json::to_value(self.cache.resources.get(&address.name)).ok()
            }
            ResourceKind::Queue => {
                serde_json::to_value(self.queue.resources.get(&address.name)).ok()
            }
            ResourceKind::Mail => serde_json::to_value(self.mail.resources.get(&address.name)).ok(),
            ResourceKind::ObjectStore => {
                serde_json::to_value(self.object_store.resources.get(&address.name)).ok()
            }
            ResourceKind::EventBus => {
                serde_json::to_value(self.event_bus.resources.get(&address.name)).ok()
            }
            ResourceKind::Scheduler => {
                serde_json::to_value(self.scheduler.resources.get(&address.name)).ok()
            }
            ResourceKind::Http => serde_json::to_value(self.http.resources.get(&address.name)).ok(),
        }
    }

    pub(crate) fn to_records(&self) -> StackRecords {
        let resources = self
            .resources
            .iter()
            .cloned()
            .map(|address| {
                let state = match address.kind {
                    ResourceKind::Db => {
                        ResourceState::Db(self.db.resources.get(&address.name).cloned())
                    }
                    ResourceKind::Cache => {
                        ResourceState::Cache(self.cache.resources.get(&address.name).cloned())
                    }
                    ResourceKind::Queue => {
                        ResourceState::Queue(self.queue.resources.get(&address.name).cloned())
                    }
                    ResourceKind::Mail => {
                        ResourceState::Mail(self.mail.resources.get(&address.name).cloned())
                    }
                    ResourceKind::ObjectStore => ResourceState::ObjectStore(
                        self.object_store.resources.get(&address.name).cloned(),
                    ),
                    ResourceKind::EventBus => ResourceState::EventBus(
                        self.event_bus.resources.get(&address.name).cloned(),
                    ),
                    ResourceKind::Scheduler => ResourceState::Scheduler(
                        self.scheduler.resources.get(&address.name).cloned(),
                    ),
                    ResourceKind::Http => {
                        ResourceState::Http(self.http.resources.get(&address.name).cloned())
                    }
                };
                (address, state)
            })
            .collect();
        let replays = self
            .replays
            .iter()
            .cloned()
            .map(|record| (replay_storage_key(&record), record))
            .collect();
        let journal = self
            .journal
            .entries()
            .iter()
            .cloned()
            .map(|entry| (entry.sequence, entry))
            .collect();

        StackRecords {
            resources,
            replays,
            journal,
            counters: StackCounters {
                next_command: self.next_command,
                next_event: self.next_event,
                queue_message: self.queue.next_message,
                queue_receipt: self.queue.next_receipt,
                mail_message: self.mail.next_message,
                scheduler_task: self.scheduler.next_task,
            },
            clocks: StackClocks {
                current: self.clock,
                initial: self.initial_clock,
            },
        }
    }

    pub(crate) fn from_records(records: StackRecords) -> Result<Self, String> {
        let mut replays = records.replays.into_values().collect::<Vec<_>>();
        replays.sort_by_key(|record| record.receipt.command_id);
        let mut snapshot = Self {
            clock: records.clocks.current,
            initial_clock: records.clocks.initial,
            resources: records.resources.keys().cloned().collect(),
            db: DbEngine::default(),
            cache: CacheEngine::default(),
            queue: QueueEngine::default(),
            mail: MailEngine::default(),
            object_store: ObjectEngine::default(),
            event_bus: BusEngine::default(),
            scheduler: SchedulerEngine::default(),
            http: HttpEngine::default(),
            journal: Journal::from_entries(records.journal.into_values().collect()),
            next_command: records.counters.next_command,
            next_event: records.counters.next_event,
            replays,
        };
        snapshot.queue.next_message = records.counters.queue_message;
        snapshot.queue.next_receipt = records.counters.queue_receipt;
        snapshot.mail.next_message = records.counters.mail_message;
        snapshot.scheduler.next_task = records.counters.scheduler_task;

        for (address, state) in records.resources {
            let name = address.name;
            match (address.kind, state) {
                (ResourceKind::Db, ResourceState::Db(value)) => {
                    if let Some(value) = value {
                        snapshot.db.resources.insert(name, value);
                    }
                }
                (ResourceKind::Cache, ResourceState::Cache(value)) => {
                    if let Some(value) = value {
                        snapshot.cache.resources.insert(name, value);
                    }
                }
                (ResourceKind::Queue, ResourceState::Queue(value)) => {
                    if let Some(value) = value {
                        snapshot.queue.resources.insert(name, value);
                    }
                }
                (ResourceKind::Mail, ResourceState::Mail(value)) => {
                    if let Some(value) = value {
                        snapshot.mail.resources.insert(name, value);
                    }
                }
                (ResourceKind::ObjectStore, ResourceState::ObjectStore(value)) => {
                    if let Some(value) = value {
                        snapshot.object_store.resources.insert(name, value);
                    }
                }
                (ResourceKind::EventBus, ResourceState::EventBus(value)) => {
                    if let Some(value) = value {
                        snapshot.event_bus.resources.insert(name, value);
                    }
                }
                (ResourceKind::Scheduler, ResourceState::Scheduler(value)) => {
                    if let Some(value) = value {
                        snapshot.scheduler.resources.insert(name, value);
                    }
                }
                (ResourceKind::Http, ResourceState::Http(value)) => {
                    if let Some(value) = value {
                        snapshot.http.resources.insert(name, value);
                    }
                }
                (kind, _) => {
                    return Err(format!(
                        "persisted resource state does not match `{kind}.{name}`"
                    ));
                }
            }
        }
        Ok(snapshot)
    }
}

pub(crate) fn replay_storage_key(record: &ReplayRecord) -> String {
    let resource = record.resource.to_string();
    format!("{:016x}:{resource}{}", resource.len(), record.key)
}
