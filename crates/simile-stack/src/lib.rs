#![forbid(unsafe_code)]

//! Deterministic capability emulators for state and effects behind Simile services.
//!
//! [`Stack`] coordinates named resources through one versioned command protocol,
//! authoritative logical clock, snapshot, reset, idempotency, and causal journal.

mod cache;
mod db;
mod error;
mod event_bus;
mod http;
mod journal;
mod mail;
mod object_store;
#[cfg(feature = "process")]
pub mod process;
mod protocol;
mod queue;
mod scheduler;
mod stack;
mod store;
mod time;

pub use cache::{CacheCommand, CacheEntry, CacheResult};
pub use db::{DbCommand, DbFilter, DbMutation, DbResult, StoredDocument, WriteCondition};
pub use error::{ErrorCode, ResourceError, Result};
pub use event_bus::{BusCommand, BusEvent, BusResult};
pub use http::{
    HeaderMap, HttpCommand, HttpExpectation, HttpRequest, HttpResponse, HttpResult, RecordedRequest,
};
pub use journal::{EventQuery, Journal, JournalEntry, JournalOutcome};
pub use mail::{MailAttachment, MailCommand, MailMessage, MailResult, MailState};
pub use object_store::{
    ObjectCommand, ObjectMetadata, ObjectResult, ObjectWriteCondition, StoredObject,
};
pub use protocol::{
    CommandContext, CommandEnvelope, CommandId, CommandMetadata, CommandReceipt, ComponentManifest,
    PROTOCOL_VERSION, ResourceAddress, StackCommand, StackResult,
};
pub use queue::{QueueCommand, QueueMessage, QueueResult};
pub use scheduler::{ScheduledTask, SchedulerCommand, SchedulerResult, TaskState};
pub use stack::{AdvanceReport, Stack, StackBuilder, StackSnapshot};
#[cfg(feature = "persistence")]
pub use store::RedbStateStore;
pub use store::{MemoryStateStore, StateIdentity, StateStore, StateStoreError};
pub use time::{Clock, ClockError, ClockMode, ClockSnapshot, LogicalTime};
