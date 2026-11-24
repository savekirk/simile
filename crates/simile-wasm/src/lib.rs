#![forbid(unsafe_code)]

//! Browser host. Application behavior belongs in `simile-core`; this crate owns
//! JavaScript bindings and enables the browser entropy provider.

mod boundary;
mod session;
pub use session::BrowserSession;

use boundary::{
    BundleFiles, ReportView, Result, artifact_error, core_error, error, from_js, json, seed, to_js,
};
use serde::{Deserialize, Serialize};
use simile_core::{ops, workspace};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ArtifactHandle {
    bytes: Vec<u8>,
    artifact: simile_artifact::Artifact,
    metadata: Option<workspace::WorkspaceMetadata>,
}

#[wasm_bindgen(js_name = compileWorkspace)]
pub fn compile_workspace(input: JsValue) -> Result<ArtifactHandle> {
    let request = from_js(input)?;
    let compiled = workspace::compile_workspace(&request).map_err(core_error)?;
    Ok(ArtifactHandle {
        bytes: compiled.bytes,
        artifact: compiled.artifact,
        metadata: Some(compiled.metadata),
    })
}

#[wasm_bindgen(js_name = loadArtifact)]
pub fn load_artifact(bytes: &[u8]) -> Result<ArtifactHandle> {
    let artifact = simile_artifact::decode(bytes).map_err(artifact_error)?;
    Ok(ArtifactHandle {
        bytes: bytes.to_vec(),
        artifact,
        metadata: None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunOptions {
    selector: Selector,
    seed: Option<String>,
    outcome: Option<String>,
    scenario: Option<String>,
    request_doc_json: Option<String>,
    content_type: Option<String>,
    bundle_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Selector {
    Key(String),
    Id(String),
}
impl Selector {
    fn operation(&self) -> ops::OperationSelector {
        match self {
            Self::Key(value) => ops::OperationSelector::Key(value.clone()),
            Self::Id(value) => ops::OperationSelector::Id(value.clone()),
        }
    }
}

#[wasm_bindgen]
impl ArtifactHandle {
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn view(&self) -> Result<JsValue> {
        let summary = self.artifact.summary();
        let name = self
            .metadata
            .as_ref()
            .map_or(summary.service.as_str(), |metadata| {
                metadata.service.as_str()
            });
        let mut catalogue =
            serde_json::to_value(ops::describe_artifact(&self.artifact, name, true))
                .map_err(|value| error("serialization_failed", value))?;
        if let Some(value) = catalogue.pointer_mut("/webhooks/reorderWindowMilliseconds") {
            *value = serde_json::Value::String(value.to_string());
        }
        let routes = self.artifact.operations().map(|operation| serde_json::json!({
            "key": operation.key(), "method": operation.method(), "path": operation.path(), "operationId": operation.operation_id(),
            "parameters": operation.parameters().map(|parameter| serde_json::json!({
                "name": parameter.name(), "location": parameter.location(), "required": parameter.is_required(),
                "style": parameter.style(), "explode": parameter.explode(), "allowReserved": parameter.allow_reserved(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>();
        to_js(&serde_json::json!({
            "revision": summary.payload_sha256, "formatVersion": summary.format_version,
            "service": summary.service, "serviceVersion": summary.service_version,
            "operations": summary.operations, "catalogue": catalogue, "routes": routes,
            "audit": ops::audit_artifact(&self.artifact).map_err(core_error)?, "metadata": self.metadata,
        }))
    }

    #[wasm_bindgen(js_name = runOperation)]
    pub fn run_operation(&self, input: JsValue) -> Result<JsValue> {
        let options: RunOptions = from_js(input)?;
        let seed = seed(options.seed.as_deref())?;
        let report = ops::run_artifact(
            self.artifact.clone(),
            &ops::BaselineRequest {
                operation: &options.selector.operation(),
                seed,
                outcome: options.outcome.as_deref(),
                scenario: options.scenario.as_deref(),
                content_type: options.content_type.as_deref(),
                request: options
                    .request_doc_json
                    .as_deref()
                    .map(|value| ("requestDocJson", value.as_bytes())),
            },
        )
        .map_err(core_error)?;
        let bundle = options
            .bundle_name
            .map(|name| {
                ops::response_bundle(
                    name.into(),
                    &self.bytes,
                    &report,
                    options.outcome.as_deref(),
                    options.scenario.as_deref(),
                )
            })
            .transpose()
            .map_err(core_error)?
            .map(Into::into);
        let mut headers = report.request.headers.clone();
        if !headers
            .keys()
            .any(|name| name.eq_ignore_ascii_case("content-type"))
        {
            if let Some(content_type) = &report.request.content_type {
                headers.insert("content-type".to_owned(), content_type.clone());
            }
        }
        let mut document =
            serde_json::Map::from_iter([("headers".to_owned(), serde_json::json!(headers))]);
        if let Some(body) = &report.request.body {
            document.insert("body".to_owned(), body.clone());
        }
        to_js(&ReportView {
            report_json: json(&report)?,
            seed: Some(seed.to_string()),
            request_doc_json: Some(json(&document)?),
            body_json: report.response.body.as_ref().map(json).transpose()?,
            bundle,
            matched: None,
        })
    }

    #[wasm_bindgen(js_name = synthesizeRequest)]
    pub fn synthesize_request(&self, input: JsValue) -> Result<JsValue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Options {
            selector: Selector,
            seed: Option<String>,
            content_type: Option<String>,
        }
        let options: Options = from_js(input)?;
        let seed = seed(options.seed.as_deref())?;
        let selected = match &options.selector {
            Selector::Key(value) => self.artifact.operation(value),
            Selector::Id(value) => self.artifact.operation_by_id(value),
        }
        .ok_or_else(|| error("unknown_operation", "operation selector did not match"))?;
        let body = if selected.request_body().is_some() {
            Some(
                simile_runtime::synthesize_request_body_with_seed(
                    &self.artifact,
                    simile_runtime::SynthesisRequest {
                        operation: selected.key(),
                        content_type: options.content_type.as_deref(),
                    },
                    seed,
                )
                .map_err(|value| error(value.code(), &value))?,
            )
        } else {
            None
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Synthesized {
            seed: String,
            request_doc_json: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            content_type: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            body: Option<serde_bytes::ByteBuf>,
        }
        let mut document = serde_json::Map::new();
        if let Some(body) = &body {
            document.insert(
                "headers".to_owned(),
                serde_json::json!({"content-type": body.media_type()}),
            );
            document.insert("body".to_owned(), body.value().clone());
        }
        to_js(&Synthesized {
            seed: seed.to_string(),
            request_doc_json: json(&document)?,
            content_type: body.as_ref().map(|body| body.media_type().to_owned()),
            body: body.map(|body| body.bytes().to_vec().into()),
        })
    }
}

#[wasm_bindgen(js_name = replayBundle)]
pub fn replay_bundle(input: JsValue) -> Result<JsValue> {
    let bundle: BundleFiles = from_js(input)?;
    let report = ops::replay_bundle(&bundle.into()).map_err(core_error)?;
    to_js(&ReportView {
        report_json: json(&report)?,
        seed: Some(report.seed.to_string()),
        matched: Some(report.matched),
        ..Default::default()
    })
}

#[wasm_bindgen(js_name = runConformance)]
pub fn run_conformance(input: JsValue) -> Result<JsValue> {
    let bundles: Vec<BundleFiles> = from_js(input)?;
    let report = ops::verify_bundles(&bundles.into_iter().map(Into::into).collect::<Vec<_>>())
        .map_err(core_error)?;
    to_js(
        &serde_json::json!({ "reportJson": json(&report)?, "total": report.total, "passed": report.passed, "failed": report.failed, "cases": report.cases }),
    )
}
