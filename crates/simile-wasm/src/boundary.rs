use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

pub type Result<T> = std::result::Result<T, JsValue>;

#[derive(Serialize)]
pub struct Diagnostic<'a> {
    pub code: &'a str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

pub fn error(code: &str, error: impl std::fmt::Display) -> JsValue {
    to_js(&Diagnostic {
        code,
        message: error.to_string(),
        file: None,
    })
    .unwrap_or_else(|_| JsValue::from_str("could not serialize diagnostic"))
}

pub fn core_error(value: simile_core::Error) -> JsValue {
    let code = match &value {
        simile_core::Error::Compile(simile_compiler::Error::UnsupportedSource { .. }) => {
            "unsupported_source"
        }
        simile_core::Error::Compile(_) | simile_core::Error::ServiceConfig(_) => "compile_error",
        simile_core::Error::Artifact(_) => "artifact_incompatible",
        simile_core::Error::Checkpoint(_) => "checkpoint_incompatible",
        simile_core::Error::WorkHandle => "stale_handle",
        simile_core::Error::Workspace(_) => "invalid_workspace",
        simile_core::Error::Runtime(runtime) => runtime.code(),
        _ => "operation_failed",
    };
    let file = match &value {
        simile_core::Error::Compile(
            simile_compiler::Error::UnsupportedSource { name, .. }
            | simile_compiler::Error::OpenApi { name, .. },
        ) => Some(name.clone()),
        simile_core::Error::ReadSource { path, .. } => Some(path.to_string_lossy().into_owned()),
        _ => None,
    };
    to_js(&Diagnostic {
        code,
        message: value.to_string(),
        file,
    })
    .unwrap_or_else(|_| JsValue::from_str("could not serialize diagnostic"))
}

pub fn to_js(value: &impl Serialize) -> Result<JsValue> {
    value
        .serialize(
            &serde_wasm_bindgen::Serializer::new()
                .serialize_maps_as_objects(true)
                .serialize_missing_as_null(false)
                .serialize_bytes_as_arrays(false)
                .serialize_large_number_types_as_bigints(false),
        )
        .map_err(|value| JsValue::from_str(&value.to_string()))
}

pub fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T> {
    serde_wasm_bindgen::from_value(value).map_err(|value| error("invalid_input", value))
}

pub fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|value| error("serialization_failed", value))
}

pub fn unsigned(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error(
            "invalid_decimal",
            "expected an unsigned decimal string",
        ));
    }
    value
        .parse()
        .map_err(|value| error("invalid_decimal", value))
}

pub fn signed(value: &str) -> Result<i64> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error("invalid_decimal", "expected a signed decimal string"));
    }
    value
        .parse()
        .map_err(|value| error("invalid_decimal", value))
}

pub fn seed(value: Option<&str>) -> Result<u64> {
    value.map_or_else(|| Ok(rand::random()), unsigned)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleFiles {
    pub name: String,
    pub files: std::collections::BTreeMap<String, serde_bytes::ByteBuf>,
}
impl From<simile_core::ops::MemoryBundle> for BundleFiles {
    fn from(value: simile_core::ops::MemoryBundle) -> Self {
        Self {
            name: value.name.to_string_lossy().into_owned(),
            files: value
                .files
                .into_iter()
                .map(|(name, bytes)| (name, bytes.into()))
                .collect(),
        }
    }
}
impl From<BundleFiles> for simile_core::ops::MemoryBundle {
    fn from(value: BundleFiles) -> Self {
        Self {
            name: value.name.into(),
            files: value
                .files
                .into_iter()
                .map(|(name, bytes)| (name, bytes.into_vec()))
                .collect(),
        }
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub report_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_doc_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<BundleFiles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<bool>,
}
