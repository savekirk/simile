use std::collections::BTreeMap;

use simile_core::{ops, workspace};
use simile_http::{HttpOptions, RequestBody};
use simile_stack::{ClockMode, ClockSnapshot, LogicalTime};

pub const SEED: u64 = 18_446_744_073_709_551_615;

pub fn project() -> workspace::WorkspaceRequest {
    workspace::WorkspaceRequest {
        files: BTreeMap::from([
            (
                "Simfile".to_owned(),
                include_str!("../fixtures/service-described.hcl").to_owned(),
            ),
            (
                "run-orders.yaml".to_owned(),
                include_str!("../fixtures/run-orders.yaml").to_owned(),
            ),
        ]),
        config_path: Some("Simfile".to_owned()),
        contract_entry: None,
        contract_format: workspace::ContractFormat::Auto,
        modules: vec![],
        heuristic_policy: Default::default(),
    }
}

/// Used unchanged by native tests and Chromium, with fixed names, seed and clock.
pub fn exercise() -> Result<(), Box<dyn std::error::Error>> {
    let mut invalid = project();
    invalid.modules.push("override.hcl".to_owned());
    assert!(workspace::compile_workspace(&invalid).is_err());
    let mut invalid = project();
    invalid
        .files
        .insert("../escape.yaml".to_owned(), String::new());
    assert!(workspace::compile_workspace(&invalid).is_err());
    let mut invalid = project();
    invalid.files.insert("./Simfile".to_owned(), String::new());
    assert!(workspace::compile_workspace(&invalid).is_err());
    let compiled = workspace::compile_workspace(&project())?;
    let description = ops::describe_artifact(&compiled.artifact, "orders", true);
    assert_eq!(
        description.description.as_deref(),
        Some("An emulator for order integration tests.")
    );
    assert_eq!(description.principal_setups.len(), 1);
    let audit = ops::audit_artifact(&compiled.artifact)?;
    assert_eq!(audit.operations, 3);
    let selector = ops::OperationSelector::Id("createOrder".to_owned());
    let report = ops::run_artifact(
        compiled.artifact.clone(),
        &ops::BaselineRequest {
            operation: &selector,
            outcome: None,
            scenario: None,
            request: None,
            content_type: None,
            seed: SEED,
        },
    )?;
    assert_eq!(report.seed, SEED);
    assert_eq!(report.response.status, 201);
    assert_eq!(
        report
            .response
            .body
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("ord_simile")
    );
    let bundle = ops::response_bundle("orders-case".into(), &compiled.bytes, &report, None, None)?;
    assert!(ops::replay_bundle(&bundle)?.matched);
    assert!(ops::verify_bundle(&bundle)?.reproduced);
    assert_eq!(ops::verify_bundles(&[bundle])?.passed, 1);
    let session = simile_core::session::ServiceSession::with_options(
        &compiled.bytes,
        ClockSnapshot {
            mode: ClockMode::Manual,
            now: LogicalTime::from_unix_millis(1_700_000_000_000),
        },
        HttpOptions {
            response_seed: Some(SEED),
            body_limit: 128,
            ..Default::default()
        },
    )?;
    let setup = session.service().setup_profile_with_seed(
        Some("merchant"),
        Some("fixture"),
        BTreeMap::from([
            ("product".to_owned(), serde_json::json!("fixture")),
            ("quantity".to_owned(), serde_json::json!(2)),
        ]),
        true,
        SEED,
    )?;
    assert_eq!(setup.profile.created_at, 1_700_000_000_000);
    assert_eq!(session.service().profiles()?.len(), 1);
    session.advance(250)?;
    assert_eq!(
        session.service().clock()?.now.as_unix_millis(),
        1_700_000_000_250
    );
    for (method, uri, body, expected) in [
        (
            "POST",
            "/orders",
            br#"{"product":"fixture","quantity":2}"#.as_slice(),
            201,
        ),
        ("POST", "/orders", b"{", 400),
        ("POST", "/orders", b"{}", 422),
        ("PUT", "/orders", b"", 405),
        ("HEAD", "/orders", b"", 405),
        ("GET", "/missing", b"", 404),
        ("HEAD", "/missing", b"", 404),
        ("GET", "/health?tag=one&tag=two", b"", 204),
    ] {
        let request = simile_http::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-repeated", "first")
            .header("x-repeated", "second")
            .body(RequestBody::Bytes(body.to_vec()))?;
        let exchange = session.handle_request(request);
        assert_eq!(
            exchange.response.status().as_u16(),
            expected,
            "{method} {uri}"
        );
        if method == "HEAD" {
            assert!(exchange.response.body().is_empty());
        }
        if expected == 405 {
            assert_eq!(exchange.response.headers()["allow"], "POST");
        }
    }
    let oversized = simile_http::http::Request::builder()
        .method("POST")
        .uri("/orders")
        .header("content-type", "application/json")
        .body(RequestBody::Bytes(vec![b' '; 129]))?;
    assert_eq!(session.handle_request(oversized).response.status(), 413);
    let state = session.inspect(&ops::StateRequest {
        service: "orders".to_owned(),
        resource: None,
        causation_id: None,
        correlation_id: None,
        after_sequence: 0,
        limit: 100,
        include_values: true,
    })?;
    assert!(state.resources.iter().any(|name| name == "queue.orders"));
    assert!(!state.events.is_empty());
    let mut typescript = project();
    typescript.config_path = None;
    typescript.contract_entry = Some("entry.d.ts".to_owned());
    typescript.files = BTreeMap::from([
        ("entry.d.ts".to_owned(), "import type { Order } from './types'; export interface OrdersOperations { readonly get: { readonly method: 'GET'; readonly path: '/'; readonly responses: { readonly '200': { readonly content: { readonly 'application/json': Order } } } } }".to_owned()),
        ("types.d.ts".to_owned(), "import type { OrdersOperations } from './entry'; export interface Order { readonly id: string; }".to_owned()),
        ("excluded.test.d.ts".to_owned(), "not TypeScript".to_owned()),
    ]);
    workspace::compile_workspace(&typescript)?;
    typescript.contract_entry = Some(".".to_owned());
    workspace::compile_workspace(&typescript)?;
    Ok(())
}
