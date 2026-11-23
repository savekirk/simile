use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use simile_http::webhook_signature;
use simile_http::{order_leases, retry_delay};
use simile_service::{DeferredWork, DependencyReadiness, Service, WorkLease};
use simile_spec::WebhookSigning;
use thiserror::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) async fn run(service: Arc<Service>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    loop {
        interval.tick().await;
        let service = Arc::clone(&service);
        match tokio::task::spawn_blocking(move || dispatch_batch(&service)).await {
            Ok(Ok(report)) if report.claimed > 0 => tracing::debug!(
                target: "simile::work",
                claimed = report.claimed,
                completed = report.completed,
                retried = report.retried,
                deferred = report.deferred,
                dead_lettered = report.dead_lettered,
                "processed deferred work"
            ),
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::error!(target: "simile::work", %error, "deferred work dispatcher failed")
            }
            Err(error) => {
                tracing::error!(target: "simile::work", %error, "deferred work dispatcher stopped unexpectedly")
            }
        }
    }
}

#[derive(Default)]
struct DispatchReport {
    claimed: usize,
    completed: usize,
    retried: usize,
    deferred: usize,
    dead_lettered: usize,
}

fn dispatch_batch(service: &Service) -> Result<DispatchReport, DeliveryError> {
    let concurrency = usize::from(service.artifact().webhook_policy().concurrency).max(1);
    let leases = order_leases(service, service.lease_work(concurrency, None)?);
    let mut report = DispatchReport {
        claimed: leases.len(),
        ..DispatchReport::default()
    };
    let agent = ureq::Agent::new_with_defaults();
    for lease in leases {
        match simile_http::prepare_lease(service, &lease)? {
            DependencyReadiness::Ready => {}
            DependencyReadiness::Waiting => {
                report.deferred += 1;
                continue;
            }
            DependencyReadiness::Blocked => {
                report.dead_lettered += 1;
                tracing::error!(
                    target: "simile::work",
                    work_id = lease.id(),
                    name = lease.work().name(),
                    "deferred work has a failed causal prerequisite"
                );
                continue;
            }
        }
        match dispatch(service, &agent, &lease) {
            Ok(()) => {
                service.acknowledge_work(&lease)?;
                report.completed += 1;
            }
            Err(error) => {
                let retrying = service.retry_work(&lease, retry_delay(lease.attempt()))?;
                if retrying {
                    report.retried += 1;
                    tracing::warn!(
                        target: "simile::work",
                        work_id = lease.id(),
                        name = lease.work().name(),
                        attempt = lease.attempt(),
                        %error,
                        "deferred work will be retried"
                    );
                } else {
                    report.dead_lettered += 1;
                    tracing::error!(
                        target: "simile::work",
                        work_id = lease.id(),
                        name = lease.work().name(),
                        attempts = lease.attempt(),
                        %error,
                        "deferred work exhausted its retry budget"
                    );
                }
            }
        }
    }
    Ok(report)
}

fn dispatch(
    service: &Service,
    agent: &ureq::Agent,
    lease: &WorkLease,
) -> Result<(), DeliveryError> {
    match lease.work() {
        DeferredWork::Async { .. } => service.execute_work(lease).map_err(Into::into),
        DeferredWork::Webhook {
            method,
            content_type,
            target,
            body,
            profile,
            ..
        } => {
            let signing = service.webhook_signing_for(profile.as_deref())?;
            deliver_webhook(
                agent,
                signing.as_ref(),
                method,
                content_type.as_deref(),
                target,
                body,
            )
        }
    }
}

fn deliver_webhook(
    agent: &ureq::Agent,
    signing: Option<&WebhookSigning>,
    method: &str,
    content_type: Option<&str>,
    target: &str,
    body: &serde_json::Value,
) -> Result<(), DeliveryError> {
    let timestamp = if signing.is_some() {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(DeliveryError::Clock)?
            .as_secs()
    } else {
        0
    };
    let request =
        simile_http::prepare_webhook(signing, method, content_type, target, body, timestamp)?;
    let response = agent.run(request).map_err(DeliveryError::Transport)?;
    if !response.status().is_success() {
        return Err(DeliveryError::Status(response.status().as_u16()));
    }
    Ok(())
}

#[derive(Debug, Error)]
enum DeliveryError {
    #[error(transparent)]
    Service(#[from] simile_service::Error),
    #[error(transparent)]
    Preparation(#[from] simile_http::WebhookError),
    #[error("webhook transport failed: {0}")]
    Transport(ureq::Error),
    #[error("webhook endpoint returned HTTP {0}")]
    Status(u16),
    #[error("system clock is before the Unix epoch: {0}")]
    Clock(std::time::SystemTimeError),
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use simile_spec::WebhookSigning;

    use super::{deliver_webhook, retry_delay, webhook_signature};

    #[test]
    fn retry_delay_grows_and_is_bounded() {
        assert_eq!(retry_delay(1), 2_000);
        assert_eq!(retry_delay(2), 4_000);
        assert_eq!(retry_delay(20), 60_000);
    }

    #[test]
    fn signatures_are_stable_for_one_timestamp_and_body() {
        assert_eq!(
            webhook_signature(b"secret", 1_700_000_000, br#"{"id":"ord_42"}"#),
            "fbbea5de96730ac365c91a8731570f69ffed16e1d727eb80ae028a3dbab8aebb"
        );
    }

    #[test]
    fn delivers_signed_json_to_the_declared_target() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|error| panic!("{error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("{error}"));
        let receiver = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!("{error}"));
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1_024];
            let header_end = loop {
                let read = stream
                    .read(&mut buffer)
                    .unwrap_or_else(|error| panic!("{error}"));
                assert_ne!(read, 0, "request ended before its headers");
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            while bytes.len() - header_end < content_length {
                let read = stream
                    .read(&mut buffer)
                    .unwrap_or_else(|error| panic!("{error}"));
                assert_ne!(read, 0, "request ended before its body");
                bytes.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap_or_else(|error| panic!("{error}"));
            String::from_utf8(bytes).unwrap_or_else(|error| panic!("{error}"))
        });
        let signing = WebhookSigning {
            secret: "whsec_local".to_owned(),
            header: "X-Simile-Signature".to_owned(),
            timestamp_header: "X-Simile-Timestamp".to_owned(),
        };

        deliver_webhook(
            &ureq::Agent::new_with_defaults(),
            Some(&signing),
            "POST",
            Some("application/json"),
            &format!("http://{address}/hooks"),
            &serde_json::json!({ "id": "evt_123" }),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let request = receiver
            .join()
            .unwrap_or_else(|_| panic!("receiver failed"));
        let request_lower = request.to_ascii_lowercase();

        assert!(request.starts_with("POST /hooks HTTP/1.1\r\n"));
        assert!(request_lower.contains("content-type: application/json"));
        assert!(request_lower.contains("x-simile-timestamp:"));
        assert!(request_lower.contains("x-simile-signature: t="));
        assert!(request.contains(r#"{"id":"evt_123"}"#));
    }
}
