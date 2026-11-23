#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]

use simile_compiler::{Compiler, ContractSource, Source, SourceCompileRequest};
use simile_runtime::{ExecutionOptions, OperationCall, Runtime};
use simile_stack::{Clock, ClockMode, ClockSnapshot, LogicalTime};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[path = "../../simile-core/tests/support/parity.rs"]
mod parity;

#[path = "../../simile-http/tests/support/parity.rs"]
mod http_parity;

#[wasm_bindgen_test]
fn shared_http_bytes_and_signatures() -> Result<(), Box<dyn std::error::Error>> {
    let service = simile_service::Service::memory_with_clock(
        http_parity::artifact()?,
        ClockSnapshot {
            mode: ClockMode::Manual,
            now: LogicalTime::from_unix_millis(1_700_000_000_000),
        },
    )?;
    http_parity::exercise(&service)
}

#[wasm_bindgen_test]
fn shared_workflows_and_http() -> Result<(), Box<dyn std::error::Error>> {
    parity::exercise()
}

#[wasm_bindgen_test]
fn compile_decode_and_execute() -> Result<(), Box<dyn std::error::Error>> {
    let service = Compiler::new().compile_source(SourceCompileRequest {
        contract: ContractSource::OpenApi(Source::new(
            "orders.yaml",
            include_str!("../../simile-core/tests/fixtures/run-orders.yaml"),
        )),
        modules: vec![Source::new(
            "orders.hcl",
            include_str!("../../simile-core/tests/fixtures/orders.hcl"),
        )],
        heuristic_policy: Default::default(),
    })?;
    let bytes = simile_artifact::encode(&service)?;
    let artifact = simile_artifact::decode(&bytes)?;
    let runtime = Runtime::new(&artifact);
    let call = OperationCall {
        operation: "POST /orders",
        content_type: Some("application/json"),
        body: br#"{"product":"fixture","quantity":2}"#,
    };
    let options = ExecutionOptions {
        response_seed: Some(u64::MAX),
        ..Default::default()
    };
    assert_eq!(
        runtime.execute_with_options(call, options)?,
        Runtime::new(&artifact).execute_with_options(call, options)?
    );
    let service = simile_service::Service::memory_with_clock(
        artifact,
        ClockSnapshot {
            mode: ClockMode::Manual,
            now: LogicalTime::UNIX_EPOCH,
        },
    )?;
    service.execute_with_options(call, options)?;
    service.snapshot()?;
    let mut manual = Clock::manual(LogicalTime::UNIX_EPOCH);
    assert_eq!(manual.advance(100)?.as_unix_millis(), 100);
    Clock::realtime(LogicalTime::UNIX_EPOCH).now()?;
    Ok(())
}

#[wasm_bindgen_test]
fn parses_multiple_typescript_files() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = Compiler::new().compile_source(SourceCompileRequest {
        contract: ContractSource::TypeScript(vec![
            Source::new(
                "types.d.ts",
                "export interface Order { readonly id: string; }",
            ),
            Source::new(
                "operations.d.ts",
                r#"
                import type { Order } from "./types";
                export type ServiceName = "Orders";
                export interface OrdersOperations {
                    readonly createOrder: {
                        readonly method: "POST";
                        readonly path: "/orders";
                        readonly responses: { readonly "201": {
                            readonly content: { readonly "application/json": Order };
                        }};
                    };
                }
            "#,
            ),
        ]),
        modules: vec![],
        heuristic_policy: Default::default(),
    })?;
    let artifact = simile_artifact::decode(&simile_artifact::encode(&compiled)?)?;
    assert!(artifact.operation_by_id("createOrder").is_some());
    Ok(())
}

#[wasm_bindgen_test]
fn smithy_source_has_an_actionable_diagnostic() {
    let result = Compiler::new().compile_source(SourceCompileRequest {
        contract: ContractSource::Smithy(Source::new("service.smithy", "")),
        modules: vec![],
        heuristic_policy: Default::default(),
    });
    assert!(matches!(
        result,
        Err(simile_compiler::Error::UnsupportedSource {
            format: "Smithy",
            ..
        })
    ));
}
