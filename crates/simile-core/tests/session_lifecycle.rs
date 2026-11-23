use simile_core::{session::ServiceSession, workspace};
use simile_http::{RequestBody, http};

fn compiled() -> Result<workspace::CompiledWorkspace, Box<dyn std::error::Error>> {
    Ok(workspace::compile_workspace(
        &workspace::WorkspaceRequest {
            files: std::collections::BTreeMap::from([
                (
                    "lifecycle.yaml".to_owned(),
                    include_str!("../../simile-wasm/tests/fixtures/lifecycle.yaml").to_owned(),
                ),
                (
                    "lifecycle.hcl".to_owned(),
                    include_str!("../../simile-wasm/tests/fixtures/lifecycle.hcl").to_owned(),
                ),
            ]),
            config_path: None,
            contract_entry: Some("lifecycle.yaml".to_owned()),
            contract_format: workspace::ContractFormat::Auto,
            modules: vec!["lifecycle.hcl".to_owned()],
            heuristic_policy: Default::default(),
        },
    )?)
}
fn request() -> Result<http::Request<RequestBody>, http::Error> {
    http::Request::builder()
        .method("POST")
        .uri("/orders")
        .header("idempotency-key", "same")
        .body(RequestBody::Bytes(Vec::new()))
}

#[test]
fn checkpoint_restores_stack_with_fresh_runtime_and_invalid_handles()
-> Result<(), Box<dyn std::error::Error>> {
    let compiled = compiled()?;
    let mut session = ServiceSession::new(&compiled.bytes)?;
    let first = session
        .handle_request_with_seed(request()?, Some(1))
        .response
        .into_body();
    assert_eq!(
        first,
        session
            .handle_request_with_seed(request()?, Some(2))
            .response
            .into_body()
    );
    let work = session.step_work(10)?;
    assert!(!work.is_empty());
    let checkpoint = session.export_checkpoint()?;
    let saved_snapshot = session.service().snapshot()?;
    session.restore_checkpoint(&checkpoint)?;
    assert_eq!(session.service().snapshot()?, saved_snapshot);
    assert!(session.acknowledge_work(&work[0].handle).is_err());
    assert!(session.step_work(10)?.is_empty());
    session.advance(30_000)?;
    let recovered = session.step_work(10)?;
    assert!(!recovered.is_empty());
    assert_ne!(recovered[0].handle, work[0].handle);
    assert_ne!(
        first,
        session
            .handle_request_with_seed(request()?, Some(2))
            .response
            .into_body()
    );
    session.reset()?;
    assert!(session.step_work(10)?.is_empty());
    assert!(session.acknowledge_work(&recovered[0].handle).is_err());
    assert_eq!(session.service().clock()?.now.as_unix_millis(), 0);
    assert_eq!(
        first,
        session
            .handle_request_with_seed(request()?, Some(1))
            .response
            .into_body()
    );
    Ok(())
}

#[test]
fn invalid_checkpoints_preserve_the_live_session() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = compiled()?;
    let mut session = ServiceSession::new(&compiled.bytes)?;
    session.handle_request_with_seed(request()?, Some(1));
    let original = session.export_checkpoint()?;
    for field in [
        "version",
        "artifactDigest",
        "artifactFormat",
        "service",
        "stackProtocol",
        "clock",
    ] {
        let mut checkpoint: serde_json::Value = serde_json::from_slice(&original)?;
        checkpoint[field] = serde_json::json!("incompatible");
        assert!(
            session
                .restore_checkpoint(&serde_json::to_vec(&checkpoint)?)
                .is_err()
        );
        assert_eq!(session.export_checkpoint()?, original);
    }
    let mut checkpoint: serde_json::Value = serde_json::from_slice(&original)?;
    checkpoint["snapshot"]["nextEvent"] = serde_json::json!(0);
    assert!(
        session
            .restore_checkpoint(&serde_json::to_vec(&checkpoint)?)
            .is_err()
    );
    assert_eq!(session.export_checkpoint()?, original);
    assert!(session.restore_checkpoint(b"not JSON").is_err());
    assert_eq!(session.export_checkpoint()?, original);
    Ok(())
}
