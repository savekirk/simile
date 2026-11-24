use std::collections::BTreeMap;
#[cfg(feature = "native")]
use std::path::PathBuf;

use serde::Serialize;
use simile_stack::{EventQuery, JournalEntry, ResourceAddress, Stack};

use crate::Result;
#[cfg(feature = "native")]
use crate::registry::{Registry, validate_service_name};
#[cfg(feature = "native")]
use crate::{Context, Error};
#[cfg(feature = "native")]
use simile_stack::{RedbStateStore, StateIdentity, StateStore};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateRequest {
    pub service: String,
    pub resource: Option<String>,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub after_sequence: u64,
    pub limit: usize,
    pub include_values: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateReport {
    pub service: String,
    pub resources: Vec<String>,
    pub total_events: usize,
    pub events: Vec<JournalEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, serde_json::Value>,
}

#[cfg(feature = "native")]
pub fn inspect_state(ctx: &Context, request: &StateRequest) -> Result<StateReport> {
    validate_service_name(&request.service)?;
    let registered = Registry::new(ctx.simile_home()?).load(&request.service)?;
    let path = service_state_path(ctx, &request.service)?;
    if !path.exists() {
        return Err(Error::StateNotFound {
            service: request.service.clone(),
        });
    }
    let summary = registered.artifact.summary();
    let store = RedbStateStore::open_with_identity(
        path,
        StateIdentity::new(summary.service, summary.payload_sha256),
    )?;
    let snapshot = store.load()?.ok_or_else(|| Error::StateNotFound {
        service: request.service.clone(),
    })?;
    inspect_snapshot(snapshot, request)
}

/// Inspect the same complete snapshot used by durable and in-memory sessions.
pub fn inspect_snapshot(
    snapshot: simile_stack::StackSnapshot,
    request: &StateRequest,
) -> Result<StateReport> {
    let stack = Stack::from_snapshot(snapshot);
    let resource = request
        .resource
        .as_deref()
        .map(str::parse::<ResourceAddress>)
        .transpose()?;
    let events = stack
        .journal()
        .query(&EventQuery {
            resource: resource.clone(),
            causation_id: request.causation_id.clone(),
            correlation_id: request.correlation_id.clone(),
            after_sequence: request.after_sequence,
            limit: request.limit,
        })
        .into_iter()
        .cloned()
        .collect();
    let resources = stack.resources().iter().map(ToString::to_string).collect();
    let values = if request.include_values {
        let snapshot = stack.snapshot()?;
        stack
            .resources()
            .iter()
            .filter(|address| resource.as_ref().is_none_or(|wanted| wanted == *address))
            .filter_map(|address| {
                snapshot
                    .resource_state(address)
                    .map(|value| (address.to_string(), value))
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    Ok(StateReport {
        service: request.service.clone(),
        resources,
        total_events: stack.journal().entries().len(),
        events,
        values,
    })
}

#[cfg(feature = "native")]
pub fn service_state_path(ctx: &Context, service: &str) -> Result<PathBuf> {
    validate_service_name(service)?;
    Ok(ctx
        .simile_home()?
        .join("state")
        .join("services")
        .join(format!("{service}.redb")))
}
