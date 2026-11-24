#![cfg(feature = "native")]
mod support {
    pub mod parity;
}

#[test]
fn portable_workflows_match_native_adapters() -> Result<(), Box<dyn std::error::Error>> {
    use simile_core::{BuildInfo, Context, ops, workspace};
    support::parity::exercise()?;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let ctx = Context::new(
        BuildInfo::new("test", None::<String>, "test", "test"),
        &fixtures,
    );
    let native = ops::prepare_service(
        &ctx,
        &ops::StartRequest {
            service: None,
            config: Some(fixtures.join("service-described.hcl")),
            host: None,
            port: None,
        },
    )?;
    let portable = workspace::compile_workspace(&support::parity::project())?;
    assert_eq!(native.artifact_bytes(), portable.bytes);
    let native_description = ops::describe_service(
        &ctx,
        &ops::DescribeServiceRequest {
            service: None,
            config: Some(
                fixtures
                    .join("service-described.hcl")
                    .to_string_lossy()
                    .into_owned(),
            ),
            verbose: true,
        },
    )?;
    let portable_description = ops::describe_artifact(&portable.artifact, "orders", true);
    let mut json = serde_json::to_value(native_description)?;
    let object = json
        .as_object_mut()
        .ok_or("description must be an object")?;
    object.remove("listen");
    object.remove("source");
    assert_eq!(json, serde_json::to_value(portable_description)?);
    let native_run = ops::run(&ops::RunRequest {
        spec: ops::SpecSource::Path(fixtures.join("run-orders.yaml")),
        contract_format: ops::ContractFormat::Auto,
        modules: vec![],
        operation: ops::OperationSelector::Id("createOrder".to_owned()),
        outcome: None,
        scenario: None,
        request: None,
        content_type: None,
        heuristic_policy: Default::default(),
        seed: Some(support::parity::SEED),
        bundle: None,
    })?;
    let mut project = support::parity::project();
    project.config_path = None;
    project.contract_entry = Some("run-orders.yaml".to_owned());
    let artifact = workspace::compile_workspace(&project)?.artifact;
    let portable_run = ops::run_artifact(
        artifact,
        &ops::BaselineRequest {
            operation: &ops::OperationSelector::Id("createOrder".to_owned()),
            outcome: None,
            scenario: None,
            request: None,
            content_type: None,
            seed: support::parity::SEED,
        },
    )?;
    assert_eq!(native_run, portable_run);
    let path = std::env::temp_dir().join(format!("simile-portable-parity-{}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    let encoded = native.artifact_bytes();
    std::fs::write(path.join("audit.similec"), encoded)?;
    assert_eq!(
        ops::audit(&ops::AuditRequest {
            artifact: path.join("audit.similec")
        })?,
        ops::audit_artifact(&native.artifact)?
    );
    let mut request = support::parity::project();
    request.config_path = None;
    request.contract_entry = Some("run-orders.yaml".to_owned());
    let compiled = workspace::compile_workspace(&request)?;
    let bundle = ops::response_bundle(path.clone(), &compiled.bytes, &portable_run, None, None)?;
    for (name, contents) in &bundle.files {
        std::fs::write(path.join(name), contents)?;
    }
    assert_eq!(
        ops::replay(&ops::ReplayRequest {
            bundle: path.clone()
        })?,
        ops::replay_bundle(&bundle)?
    );
    assert_eq!(
        ops::verify_conformance(&ops::ConformanceRequest {
            bundles: vec![path.clone()]
        })?,
        ops::verify_bundles(&[bundle])?
    );
    let clock = simile_stack::ClockSnapshot {
        mode: simile_stack::ClockMode::Manual,
        now: simile_stack::LogicalTime::from_unix_millis(1_700_000_000_000),
    };
    let first = simile_service::Service::memory_with_clock(native.artifact, clock)?;
    let second = simile_service::Service::memory_with_clock(portable.artifact, clock)?;
    let values = std::collections::BTreeMap::from([
        ("product".to_owned(), serde_json::json!("fixture")),
        ("quantity".to_owned(), serde_json::json!(2)),
    ]);
    assert_eq!(
        first
            .setup_profile_with_seed(
                Some("merchant"),
                Some("fixture"),
                values.clone(),
                true,
                support::parity::SEED
            )?
            .profile,
        second
            .setup_profile_with_seed(
                Some("merchant"),
                Some("fixture"),
                values,
                true,
                support::parity::SEED
            )?
            .profile
    );
    assert_eq!(first.snapshot()?, second.snapshot()?);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
