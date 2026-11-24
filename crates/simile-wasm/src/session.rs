use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use simile_core::{ops, session::ServiceSession};
use simile_http::{HttpOptions, RequestBody, http};
use simile_stack::{ClockMode, ClockSnapshot, LogicalTime};
use wasm_bindgen::prelude::*;

use crate::ArtifactHandle;
use crate::boundary::{Result, core_error, error, from_js, json, seed, signed, to_js, unsigned};

#[wasm_bindgen]
pub struct BrowserSession {
    inner: ServiceSession,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionOptions {
    clock_mode: Option<ClockMode>,
    start_milliseconds: Option<String>,
    scenario: Option<String>,
    fault_profile: Option<String>,
    response_seed: Option<String>,
    body_limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpRequest {
    method: String,
    uri: String,
    #[serde(default)]
    headers: Vec<(String, serde_bytes::ByteBuf)>,
    body: Option<serde_bytes::ByteBuf>,
    response_seed: Option<String>,
    profile: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exchange {
    status: u16,
    headers: Vec<(String, serde_bytes::ByteBuf)>,
    body: serde_bytes::ByteBuf,
    seed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    transport: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    fault: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    incident: Option<serde_json::Value>,
}

fn headers(values: &http::HeaderMap) -> Vec<(String, serde_bytes::ByteBuf)> {
    values
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec().into()))
        .collect()
}

#[wasm_bindgen]
impl BrowserSession {
    #[wasm_bindgen(constructor)]
    pub fn new(artifact: &ArtifactHandle, input: JsValue) -> Result<BrowserSession> {
        let options: SessionOptions = if input.is_undefined() {
            Default::default()
        } else {
            from_js(input)?
        };
        let mode = options.clock_mode.unwrap_or(ClockMode::Manual);
        let clock = ClockSnapshot {
            mode,
            now: LogicalTime::from_unix_millis(
                options
                    .start_milliseconds
                    .as_deref()
                    .map(signed)
                    .transpose()?
                    .unwrap_or_else(|| match mode {
                        ClockMode::Manual => 0,
                        ClockMode::Realtime => js_sys::Date::now() as i64,
                    }),
            ),
        };
        let options = HttpOptions {
            scenario: options.scenario,
            fault_profile: options.fault_profile,
            response_seed: options.response_seed.as_deref().map(unsigned).transpose()?,
            body_limit: options
                .body_limit
                .map_or(simile_http::DEFAULT_BODY_LIMIT, |value| value as usize),
        };
        Ok(Self {
            inner: ServiceSession::with_options(&artifact.bytes, clock, options)
                .map_err(core_error)?,
        })
    }

    pub fn status(&self) -> Result<JsValue> {
        to_js(&self.inner.status().map_err(core_error)?)
    }

    #[wasm_bindgen(js_name = inspectWork)]
    pub fn inspect_work(&self) -> Result<JsValue> {
        let items = self
            .inner
            .service()
            .inspect_work()
            .map_err(|value| error("work_inspection", value))?;
        let items = items.into_iter().map(|item| {
            Ok(serde_json::json!({"id":item.id,"name":item.work.name(),
                "kind": match item.work { simile_service::DeferredWork::Async { .. } => "async", _ => "webhook" },
                "deadLetter":item.dead_letter,"leased":item.leased,"availableAt":item.available_at.to_string(),
                "readiness": match item.readiness { simile_service::DependencyReadiness::Ready => "ready", simile_service::DependencyReadiness::Waiting => "waiting", simile_service::DependencyReadiness::Blocked => "blocked" },
                "workJson":json(&item.work)?}))
        }).collect::<Result<Vec<_>>>()?;
        to_js(&items)
    }

    #[wasm_bindgen(js_name = redeliverDeadLetter)]
    pub fn redeliver_dead_letter(&self) -> Result<JsValue> {
        let redelivered = self.inner.redeliver_dead_letter().map_err(core_error)?;
        to_js(
            &serde_json::json!({"redelivered":redelivered,"status":self.inner.status().map_err(core_error)?}),
        )
    }

    #[wasm_bindgen(js_name = handleRequest)]
    pub fn handle_request(&self, input: JsValue) -> Result<JsValue> {
        let input: HttpRequest = from_js(input)?;
        let response_seed = input.response_seed.as_deref().map(unsigned).transpose()?;
        let mut request = http::Request::builder()
            .method(input.method.as_str())
            .uri(&input.uri);
        for (name, value) in &input.headers {
            request = request.header(name, value.as_ref());
        }
        let mut request = request
            .body(RequestBody::Bytes(
                input.body.map_or_else(Vec::new, |value| value.into_vec()),
            ))
            .map_err(|value| error("invalid_http_request", value))?;
        if let Some(profile) = input.profile {
            simile_http::apply_profile(self.inner.service(), &profile, &mut request)
                .map_err(|value| error("profile_credentials", value))?;
        }
        let exchange = self.inner.handle_request_with_seed(request, response_seed);
        let transport = match exchange.transport {
            simile_runtime::TransportPlan::Respond => serde_json::json!({"kind":"respond"}),
            simile_runtime::TransportPlan::RespondAfter { milliseconds } => {
                serde_json::json!({"kind":"delay", "milliseconds":milliseconds.to_string()})
            }
            simile_runtime::TransportPlan::DisconnectAfterCommit => {
                serde_json::json!({"kind":"disconnect_after_commit"})
            }
        };
        let fault = exchange.fault.map(|fault| serde_json::json!({"name":fault.name(), "phase":fault.phase(), "source": match fault.source() {
            simile_runtime::FaultSource::Scenario(name) => serde_json::json!({"kind":"scenario", "name":name}),
            simile_runtime::FaultSource::Profile(name) => serde_json::json!({"kind":"profile", "name":name}),
        }}));
        let incident = exchange
            .incident
            .map(|incident| serde_json::json!({"code":incident.code,"message":incident.message}));
        to_js(&Exchange {
            status: exchange.response.status().as_u16(),
            headers: headers(exchange.response.headers()),
            body: exchange.response.into_body().into(),
            seed: exchange.seed.to_string(),
            operation: exchange.operation,
            profile: exchange.profile,
            transport,
            fault,
            incident,
        })
    }

    #[wasm_bindgen(js_name = setupProfile)]
    pub fn setup_profile(&self, input: JsValue) -> Result<JsValue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Options {
            principal: Option<String>,
            name: Option<String>,
            values_json: String,
            #[serde(default)]
            select: bool,
            seed: Option<String>,
        }
        let options: Options = from_js(input)?;
        let values: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&options.values_json)
                .map_err(|value| error("invalid_json", value))?;
        let seed = seed(options.seed.as_deref())?;
        let result = self
            .inner
            .setup_profile(
                options.principal.as_deref(),
                options.name.as_deref(),
                values,
                options.select,
                seed,
            )
            .map_err(core_error)?;
        // Issued values are explicit opaque JSON. They are never copied to diagnostics.
        to_js(
            &serde_json::json!({"seed":seed.to_string(),"created":result.created,"selected":result.selected,
            "profileJson": json(&result.profile)?, "name":result.profile.name, "id":result.profile.id,
            "createdAt":result.profile.created_at.to_string(), "updatedAt":result.profile.updated_at.to_string()}),
        )
    }

    #[wasm_bindgen(js_name = listProfiles)]
    pub fn list_profiles(&self) -> Result<JsValue> {
        let profiles = self.inner.profiles().map_err(core_error)?;
        let mut values = serde_json::to_value(&profiles)
            .map_err(|value| error("serialization_failed", value))?;
        if let Some(profiles) = values.as_array_mut() {
            for profile in profiles {
                for key in ["createdAt", "updatedAt"] {
                    if let Some(value) = profile.get_mut(key) {
                        *value = serde_json::Value::String(value.to_string());
                    }
                }
            }
        }
        to_js(&values)
    }

    #[wasm_bindgen(js_name = inspectState)]
    pub fn inspect_state(&self, input: JsValue) -> Result<JsValue> {
        #[derive(Default, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Query {
            resource: Option<String>,
            causation_id: Option<String>,
            correlation_id: Option<String>,
            after_sequence: Option<String>,
            limit: Option<u32>,
            #[serde(default)]
            include_values: bool,
        }
        let query: Query = if input.is_undefined() {
            Default::default()
        } else {
            from_js(input)?
        };
        let report = self
            .inner
            .inspect(&ops::StateRequest {
                service: self.inner.service().artifact().summary().service,
                resource: query.resource,
                causation_id: query.causation_id,
                correlation_id: query.correlation_id,
                after_sequence: query
                    .after_sequence
                    .as_deref()
                    .map(unsigned)
                    .transpose()?
                    .unwrap_or(0),
                limit: query.limit.unwrap_or(100) as usize,
                include_values: query.include_values,
            })
            .map_err(core_error)?;
        // Payload-bearing resource state remains in canonical report JSON.
        let events = report.events.iter().map(|event| serde_json::json!({"sequence":event.sequence.to_string(), "commandId":event.command_id.0.to_string(),
            "at":event.at.as_unix_millis().to_string(), "resource":event.resource.to_string(), "action":event.action, "outcome":event.outcome})).collect::<Vec<_>>();
        to_js(
            &serde_json::json!({"reportJson":json(&report)?,"resources":report.resources,"totalEvents":report.total_events,"events":events}),
        )
    }

    #[wasm_bindgen(js_name = advanceClock)]
    pub fn advance_clock(&self, milliseconds: &str) -> Result<JsValue> {
        let report = self
            .inner
            .advance(unsigned(milliseconds)?)
            .map_err(core_error)?;
        to_js(
            &serde_json::json!({"clockMilliseconds":report.now.as_unix_millis().to_string(), "reportJson":json(&report)?}),
        )
    }

    #[wasm_bindgen(js_name = resetState)]
    pub fn reset_state(&mut self) -> Result<JsValue> {
        self.inner.reset().map_err(core_error)?;
        self.status()
    }

    #[wasm_bindgen(js_name = exportCheckpoint)]
    pub fn export_checkpoint(&self) -> Result<JsValue> {
        #[derive(Serialize)]
        struct Checkpoint {
            bytes: serde_bytes::ByteBuf,
            status: simile_core::session::SessionStatus,
        }
        to_js(&Checkpoint {
            bytes: self.inner.export_checkpoint().map_err(core_error)?.into(),
            status: self.inner.status().map_err(core_error)?,
        })
    }

    #[wasm_bindgen(js_name = restoreCheckpoint)]
    pub fn restore_checkpoint(&mut self, bytes: &[u8]) -> Result<JsValue> {
        self.inner.restore_checkpoint(bytes).map_err(core_error)?;
        self.status()
    }

    #[wasm_bindgen(js_name = stepWork)]
    pub fn step_work(&mut self, limit: f64) -> Result<JsValue> {
        if !limit.is_finite() || limit.fract() != 0.0 || limit < 0.0 || limit > f64::from(u32::MAX)
        {
            return Err(error(
                "invalid_input",
                "work limit must be an unsigned 32-bit integer",
            ));
        }
        to_js(&self.inner.step_work(limit as usize).map_err(core_error)?)
    }

    #[wasm_bindgen(js_name = prepareWebhook)]
    pub fn prepare_webhook(&self, handle: &str, timestamp: &str) -> Result<JsValue> {
        let request = self
            .inner
            .prepare_webhook(handle, unsigned(timestamp)?)
            .map_err(core_error)?;
        #[derive(Serialize)]
        struct Prepared {
            method: String,
            uri: String,
            headers: Vec<(String, serde_bytes::ByteBuf)>,
            body: serde_bytes::ByteBuf,
        }
        to_js(&Prepared {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            headers: headers(request.headers()),
            body: request.into_body().into(),
        })
    }

    #[wasm_bindgen(js_name = executeWork)]
    pub fn execute_work(&mut self, handle: &str) -> Result<JsValue> {
        self.inner.execute_work(handle).map_err(core_error)?;
        self.status()
    }

    #[wasm_bindgen(js_name = acknowledgeWork)]
    pub fn acknowledge_work(&mut self, handle: &str) -> Result<JsValue> {
        self.inner.acknowledge_work(handle).map_err(core_error)?;
        self.status()
    }

    #[wasm_bindgen(js_name = retryWork)]
    pub fn retry_work(&mut self, handle: &str, delay: &str) -> Result<JsValue> {
        let retrying = self
            .inner
            .retry_work(handle, unsigned(delay)?)
            .map_err(core_error)?;
        to_js(
            &serde_json::json!({"retrying":retrying,"status":self.inner.status().map_err(core_error)?}),
        )
    }
}
