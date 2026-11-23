use sha2::{Digest as _, Sha256};
use simile_runtime::{WebhookDeliveryPair, WebhookOrder, decide_webhook_order};
use simile_service::{DeferredWork, Service, WorkLease};
use simile_spec::WebhookSigning;
use thiserror::Error;
const MAX_RETRY_DELAY_MS: u64 = 60_000;
pub type PreparedWebhook = http::Request<Vec<u8>>;

/// Serialize and sign exactly the bytes the host should send. No host clock or
/// network is consulted here; callers provide the signing timestamp explicitly.
pub fn prepare_webhook(
    signing: Option<&WebhookSigning>,
    method: &str,
    content_type: Option<&str>,
    target: &str,
    body: &serde_json::Value,
    timestamp: u64,
) -> Result<PreparedWebhook, WebhookError> {
    let bytes = serde_json::to_vec(body)?;
    let mut request = http::Request::builder().method(method).uri(target);
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    if let Some(signing) = signing {
        let signature = webhook_signature(signing.secret.as_bytes(), timestamp, &bytes);
        request = request
            .header(&signing.timestamp_header, timestamp.to_string())
            .header(&signing.header, format!("t={timestamp},v1={signature}"));
    }
    Ok(request.body(bytes)?)
}

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("could not encode webhook body: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("could not build webhook request: {0}")]
    Request(#[from] http::Error),
}

pub struct ReadyWork {
    pub ready: Vec<WorkLease>,
    pub claimed: usize,
    pub deferred: usize,
    pub dead_lettered: usize,
}

/// Enforce causal readiness before exposing a lease for execution or delivery.
pub fn lease_ready_work(
    service: &Service,
    limit: usize,
) -> Result<ReadyWork, simile_service::Error> {
    let leases = order_leases(service, service.lease_work(limit, None)?);
    let mut batch = ReadyWork {
        claimed: leases.len(),
        ready: Vec::new(),
        deferred: 0,
        dead_lettered: 0,
    };
    for lease in leases {
        match prepare_lease(service, &lease)? {
            simile_service::DependencyReadiness::Ready => batch.ready.push(lease),
            simile_service::DependencyReadiness::Waiting => batch.deferred += 1,
            simile_service::DependencyReadiness::Blocked => batch.dead_lettered += 1,
        }
    }
    Ok(batch)
}

pub fn prepare_lease(
    service: &Service,
    lease: &WorkLease,
) -> Result<simile_service::DependencyReadiness, simile_service::Error> {
    let readiness = service.dependency_readiness(lease)?;
    match readiness {
        simile_service::DependencyReadiness::Ready => {}
        simile_service::DependencyReadiness::Waiting => service.defer_work(lease, 250)?,
        simile_service::DependencyReadiness::Blocked => service.dead_letter_work(lease)?,
    }
    Ok(readiness)
}

pub fn order_leases(service: &Service, mut leases: Vec<WorkLease>) -> Vec<WorkLease> {
    // First restore hard causal edges. A compact `after` declaration remains
    // authoritative even when queue availability or retries change arrival order.
    for _ in 0..leases.len() {
        let mut changed = false;
        for index in 0..leases.len().saturating_sub(1) {
            let (_, left_correlation, left_after) = webhook_identity(&leases[index]);
            let (right_name, right_correlation, _) = webhook_identity(&leases[index + 1]);
            if left_correlation == right_correlation
                && left_after.iter().any(|dependency| dependency == right_name)
            {
                leases.swap(index, index + 1);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Then allow independent adjacent deliveries to overtake according to the
    // service policy. The artifact fingerprint keeps the decision reproducible.
    for index in 0..leases.len().saturating_sub(1) {
        let (left_name, correlation, _) = webhook_identity(&leases[index]);
        let left_name = left_name.to_owned();
        let correlation = correlation.to_owned();
        let (_, right_correlation, right_after) = webhook_identity(&leases[index + 1]);
        if correlation.is_empty()
            || correlation != right_correlation
            || right_after
                .iter()
                .any(|dependency| dependency == &left_name)
        {
            continue;
        }
        let decision = decide_webhook_order(
            service.artifact(),
            WebhookDeliveryPair {
                correlation_id: &correlation,
                earlier_delivery_id: leases[index].id(),
                later_delivery_id: leases[index + 1].id(),
                separation_milliseconds: 0,
                hard_dependency: false,
            },
        );
        if decision.order == WebhookOrder::Overtake {
            leases.swap(index, index + 1);
        }
    }
    leases
}

fn webhook_identity(lease: &WorkLease) -> (&str, &str, &[String]) {
    match lease.work() {
        DeferredWork::Webhook {
            name,
            correlation_id,
            after,
            ..
        } => (name, correlation_id, after),
        DeferredWork::Async { .. } => ("", "", &[]),
    }
}

pub fn webhook_signature(secret: &[u8], timestamp: u64, body: &[u8]) -> String {
    let mut message = timestamp.to_string().into_bytes();
    message.push(b'.');
    message.extend_from_slice(body);
    hex(&hmac_sha256(secret, &message))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_key = [0x36_u8; BLOCK];
    let mut outer_key = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner_key[index] ^= normalized[index];
        outer_key[index] ^= normalized[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_key);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_key);
    outer.update(inner);
    outer.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub const fn retry_delay(attempt: u32) -> u64 {
    let exponent = if attempt > 10 { 10 } else { attempt };
    let delay = 1_000_u64.saturating_mul(1_u64 << exponent);
    if delay > MAX_RETRY_DELAY_MS {
        MAX_RETRY_DELAY_MS
    } else {
        delay
    }
}
