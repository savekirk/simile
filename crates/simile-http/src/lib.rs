#![forbid(unsafe_code)]
//! Shared HTTP semantics. Hosts own byte collection, delays, sockets, and delivery.
mod webhook;
pub use http;
use http::header::{ALLOW, CONTENT_TYPE, COOKIE};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use serde_json::json;
use simile_artifact::Artifact;
use simile_runtime::{
    ExecutionOptions, IncomingParameters, OperationCall, ReplayIdentity, Runtime, TransportPlan,
    ValidatedParameters, validate_parameters,
};
use simile_service::Service;
use std::collections::{BTreeMap, BTreeSet};
pub use webhook::{
    PreparedWebhook, ReadyWork, WebhookError, lease_ready_work, order_leases, prepare_lease,
    prepare_webhook, retry_delay, webhook_signature,
};
pub type Response = http::Response<Vec<u8>>;
pub const DEFAULT_BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Body collection results, consumed only after request-head validation.
pub enum RequestBody {
    Bytes(Vec<u8>),
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct HttpOptions {
    pub body_limit: usize,
    pub scenario: Option<String>,
    pub fault_profile: Option<String>,
    pub response_seed: Option<u64>,
}
impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            body_limit: DEFAULT_BODY_LIMIT,
            scenario: None,
            fault_profile: None,
            response_seed: None,
        }
    }
}

pub struct HttpExchange {
    pub response: Response,
    pub transport: TransportPlan,
    pub operation: Option<String>,
    pub profile: Option<String>,
    pub seed: u64,
    pub fault: Option<simile_runtime::FaultActivation>,
    pub incident: Option<RuntimeIncident>,
}

/// A validated request head bound to its service, options, and response seed.
/// Consume it once the host has collected the bounded body.
pub struct PreparedRequest<'a> {
    service: &'a Service,
    options: &'a HttpOptions,
    head: RequestHead,
    exchange: HttpExchange,
}

struct RequestHead {
    method: Method,
    operation: String,
    incoming: IncomingParameters,
    parameters: ValidatedParameters,
    content_type: Option<String>,
    idempotency_key: Option<String>,
    resource_identity: String,
    profile: Option<String>,
}

pub fn handle_request(
    service: &Service,
    routes: &RouteTable,
    request: Request<RequestBody>,
    options: &HttpOptions,
) -> HttpExchange {
    let (parts, body) = request.into_parts();
    match prepare_request_head(service, routes, &parts, options) {
        Ok(prepared) => prepared.handle_body(body),
        Err(exchange) => *exchange,
    }
}

/// Route, authenticate, and validate parameters and headers without reading a body.
/// Streaming hosts must call this before polling the request body.
pub fn prepare_request_head<'a>(
    service: &'a Service,
    routes: &RouteTable,
    parts: &http::request::Parts,
    options: &'a HttpOptions,
) -> Result<PreparedRequest<'a>, Box<HttpExchange>> {
    let mut exchange = HttpExchange {
        response: Response::new(Vec::new()),
        transport: TransportPlan::Respond,
        operation: None,
        profile: None,
        seed: options.response_seed.unwrap_or_else(rand::random),
        fault: None,
        incident: None,
    };
    match validate_head(service, routes, parts, &mut exchange) {
        Ok(head) => Ok(PreparedRequest {
            service,
            options,
            head,
            exchange,
        }),
        Err(response) => Err(Box::new(finish_exchange(
            exchange,
            *response,
            parts.method == Method::HEAD,
        ))),
    }
}

impl PreparedRequest<'_> {
    pub fn handle_body(mut self, body: RequestBody) -> HttpExchange {
        let head = self.head.method == Method::HEAD;
        let response = dispatch(
            self.service,
            self.head,
            body,
            self.options,
            &mut self.exchange,
        );
        finish_exchange(self.exchange, response, head)
    }
}

fn finish_exchange(mut exchange: HttpExchange, mut response: Response, head: bool) -> HttpExchange {
    if head {
        response.body_mut().clear();
    }
    exchange.response = response;
    exchange
}

fn validate_head(
    service: &Service,
    routes: &RouteTable,
    parts: &http::request::Parts,
    exchange: &mut HttpExchange,
) -> Result<RequestHead, Box<Response>> {
    let method = parts.method.clone();
    let (operation, path_parameters) = match routes.find(&method, parts.uri.path()) {
        Match::Operation { operation, path } => (operation.to_owned(), path),
        Match::MethodNotAllowed(methods) => return Err(Box::new(method_not_allowed(methods))),
        Match::NotFound => {
            return Err(Box::new(error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "route not found",
            )));
        }
    };
    exchange.operation = Some(operation.clone());
    let incoming = incoming_parameters(parts, path_parameters);
    let authentication = match service.authenticate(&operation, &incoming) {
        Ok(authentication) => authentication,
        Err(simile_service::Error::Runtime(error)) => {
            return Err(Box::new(operation_error(
                service.runtime(),
                &operation,
                error,
                method == Method::HEAD,
                exchange.seed,
            )));
        }
        Err(_) => {
            return Err(Box::new(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_state_unavailable",
                "service profile state is unavailable",
            )));
        }
    };
    let parameters = match validate_parameters(service.artifact(), &operation, &incoming) {
        Ok(parameters) => parameters,
        Err(error) => {
            return Err(Box::new(operation_error(
                service.runtime(),
                &operation,
                error,
                method == Method::HEAD,
                exchange.seed,
            )));
        }
    };
    let content_type = header(&parts.headers, CONTENT_TYPE.as_str())?;
    let idempotency_key = header(&parts.headers, "idempotency-key")?;
    Ok(RequestHead {
        method,
        operation,
        incoming,
        parameters,
        content_type,
        idempotency_key,
        resource_identity: parts.uri.to_string(),
        profile: authentication.profile().map(str::to_owned),
    })
}

fn dispatch(
    service: &Service,
    head: RequestHead,
    body: RequestBody,
    options: &HttpOptions,
    exchange: &mut HttpExchange,
) -> Response {
    let RequestHead {
        method,
        operation,
        incoming,
        parameters,
        content_type,
        idempotency_key,
        resource_identity,
        profile,
    } = head;
    let body = match body {
        RequestBody::Bytes(body) if body.len() <= options.body_limit => body,
        _ => {
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                "request body exceeds the configured limit",
            );
        }
    };
    let replay = idempotency_key
        .as_deref()
        .map(ReplayIdentity::IdempotencyKey)
        .or_else(|| {
            (method == Method::GET).then_some(ReplayIdentity::Resource(&resource_identity))
        });
    exchange.profile = profile;
    match service.execute_for_profile_with_options(
        OperationCall {
            operation: &operation,
            content_type: content_type.as_deref(),
            body: &body,
        },
        ExecutionOptions {
            parameters: Some(&parameters),
            replay,
            scenario: options.scenario.as_deref(),
            fault_profile: options.fault_profile.as_deref(),
            response_seed: Some(exchange.seed),
            ..ExecutionOptions::default()
        },
        exchange.profile.as_deref(),
    ) {
        Ok(execution) => {
            exchange.transport = execution.transport().clone();
            exchange.fault = execution.fault().cloned();
            operation_response(execution.response(), method == Method::HEAD)
        }
        Err(simile_service::Error::Runtime(error)) => {
            if error.is_reportable() {
                if let Some(body) = sanitize_body(content_type.as_deref(), &body) {
                    exchange.incident = Some(RuntimeIncident {
                        profile: exchange.profile.clone(),
                        operation: operation.clone(),
                        seed: exchange.seed,
                        content_type,
                        body,
                        parameters: sanitize_parameters(incoming),
                        scenario: options.scenario.clone(),
                        fault_profile: options.fault_profile.clone(),
                        code: error.code().to_owned(),
                        message: error.to_string(),
                        issues: error
                            .issues()
                            .iter()
                            .filter_map(|issue| serde_json::to_value(issue).ok())
                            .collect(),
                    });
                }
            }
            operation_error(
                service.runtime(),
                &operation,
                error,
                method == Method::HEAD,
                exchange.seed,
            )
        }
        Err(_) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "effect_failed",
            "the operation completed but its declared effects could not be committed",
        ),
    }
}

/// Sanitized evidence for one internal runtime failure that may become an automatic report.
#[derive(Clone, Debug)]
pub struct RuntimeIncident {
    pub profile: Option<String>,
    pub operation: String,
    pub seed: u64,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub parameters: IncomingParameters,
    pub scenario: Option<String>,
    pub fault_profile: Option<String>,
    pub code: String,
    pub message: String,
    pub issues: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteTable {
    routes: Vec<Route>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Route {
    method: String,
    path: PathTemplate,
    operation: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PathTemplate {
    segments: Vec<PathSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PathSegment {
    Literal(String),
    Parameter(String),
}

pub enum Match<'a> {
    Operation {
        operation: &'a str,
        path: BTreeMap<String, Vec<String>>,
    },
    MethodNotAllowed(BTreeSet<&'a str>),
    NotFound,
}

impl RouteTable {
    pub fn new(artifact: &Artifact) -> Self {
        let mut routes = artifact
            .operations()
            .map(|operation| Route {
                method: operation.method().to_owned(),
                path: PathTemplate::parse(operation.path()),
                operation: operation.key().to_owned(),
            })
            .collect::<Vec<_>>();
        routes.sort_by(|left, right| {
            right
                .path
                .literal_count()
                .cmp(&left.path.literal_count())
                .then_with(|| left.operation.cmp(&right.operation))
        });
        Self { routes }
    }

    pub fn find(&self, method: &Method, path: &str) -> Match<'_> {
        let mut allowed = BTreeSet::new();
        for route in &self.routes {
            let Some(parameters) = route.path.match_and_capture(path) else {
                continue;
            };
            if route.method == method.as_str() {
                return Match::Operation {
                    operation: &route.operation,
                    path: parameters,
                };
            }
            allowed.insert(route.method.as_str());
        }
        if allowed.is_empty() {
            Match::NotFound
        } else {
            Match::MethodNotAllowed(allowed)
        }
    }
}

impl PathTemplate {
    fn parse(path: &str) -> Self {
        let segments = split_path(path)
            .map(|segment| {
                if segment.starts_with('{') && segment.ends_with('}') {
                    PathSegment::Parameter(
                        segment
                            .trim_start_matches('{')
                            .trim_end_matches('}')
                            .to_owned(),
                    )
                } else {
                    PathSegment::Literal(segment.to_owned())
                }
            })
            .collect();
        Self { segments }
    }

    fn match_and_capture(&self, path: &str) -> Option<BTreeMap<String, Vec<String>>> {
        let actual = split_path(path).collect::<Vec<_>>();
        if self.segments.len() != actual.len() {
            return None;
        }
        let mut captured = BTreeMap::new();
        for (expected, actual) in self.segments.iter().zip(actual) {
            match expected {
                PathSegment::Literal(expected) if expected == actual => {}
                PathSegment::Literal(_) => return None,
                PathSegment::Parameter(name) if !actual.is_empty() => {
                    let decoded = percent_encoding::percent_decode_str(actual)
                        .decode_utf8()
                        .ok()?
                        .into_owned();
                    captured.insert(name.clone(), vec![decoded]);
                }
                PathSegment::Parameter(_) => return None,
            }
        }
        Some(captured)
    }

    fn literal_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| matches!(segment, PathSegment::Literal(_)))
            .count()
    }
}

fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
}

fn sanitize_parameters(mut parameters: IncomingParameters) -> IncomingParameters {
    redact_parameter_map(&mut parameters.path);
    redact_parameter_map(&mut parameters.query);
    redact_parameter_map(&mut parameters.headers);
    redact_parameter_map(&mut parameters.cookies);
    parameters
}

fn redact_parameter_map(values: &mut BTreeMap<String, Vec<String>>) {
    for (name, values) in values {
        if sensitive_name(name) {
            values.clear();
            values.push("<redacted>".to_owned());
        }
    }
}

fn sanitize_body(content_type: Option<&str>, body: &[u8]) -> Option<Vec<u8>> {
    if body.is_empty() {
        return Some(Vec::new());
    }
    let media_type = content_type?.split(';').next()?.trim();
    if media_type != "application/json" && !media_type.ends_with("+json") {
        return None;
    }
    let mut value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    redact_json(&mut value);
    serde_json::to_vec(&value).ok()
}

fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (name, value) in object {
                if sensitive_name(name) {
                    *value = serde_json::Value::String("<redacted>".to_owned());
                } else {
                    redact_json(value);
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_json),
        _ => {}
    }
}

fn sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace(['-', '.'], "_");
    [
        "authorization",
        "cookie",
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "card_number",
        "account_number",
        "cvc",
        "cvv",
    ]
    .iter()
    .any(|sensitive| normalized == *sensitive || normalized.ends_with(&format!("_{sensitive}")))
}

fn incoming_parameters(
    parts: &http::request::Parts,
    path: BTreeMap<String, Vec<String>>,
) -> IncomingParameters {
    let mut query = BTreeMap::<String, Vec<String>>::new();
    if let Some(raw) = parts.uri.query() {
        for (name, value) in form_urlencoded::parse(raw.as_bytes()) {
            query
                .entry(name.into_owned())
                .or_default()
                .push(value.into_owned());
        }
    }
    let mut headers = BTreeMap::<String, Vec<String>>::new();
    for name in parts.headers.keys() {
        let values = parts
            .headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !values.is_empty() {
            headers.insert(name.as_str().to_ascii_lowercase(), values);
        }
    }
    let mut cookies = BTreeMap::<String, Vec<String>>::new();
    for value in parts.headers.get_all(COOKIE).iter() {
        let Ok(value) = value.to_str() else {
            continue;
        };
        for pair in value.split(';') {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            cookies
                .entry(name.trim().to_owned())
                .or_default()
                .push(value.trim().to_owned());
        }
    }
    IncomingParameters {
        path,
        query,
        headers,
        cookies,
    }
}

fn header(headers: &HeaderMap, name: &str) -> std::result::Result<Option<String>, Box<Response>> {
    headers
        .get(name)
        .map(|value| {
            value.to_str().map(str::to_owned).map_err(|_| {
                Box::new(error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_header",
                    "request contains a non-text header value",
                ))
            })
        })
        .transpose()
}

fn operation_response(response: &simile_runtime::SynthesizedResponse, head: bool) -> Response {
    let status =
        StatusCode::from_u16(response.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = if head {
        Vec::new()
    } else {
        response.bytes().to_vec()
    };
    let mut outgoing = Response::new(body);
    *outgoing.status_mut() = status;
    if let Some(content_type) = response.media_type() {
        let Ok(value) = HeaderValue::from_str(content_type) else {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_response_header",
                "compiled response contains an invalid content type",
            );
        };
        outgoing.headers_mut().insert(CONTENT_TYPE, value);
    }
    if let Err(error) = apply_response_headers(&mut outgoing, response) {
        return *error;
    }
    outgoing
}

fn apply_response_headers(
    outgoing: &mut Response,
    response: &simile_runtime::SynthesizedResponse,
) -> std::result::Result<(), Box<Response>> {
    for (name, value) in response.headers() {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            Box::new(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_response_header",
                "compiled response contains an invalid header name",
            ))
        })?;
        let value = HeaderValue::from_str(value).map_err(|_| {
            Box::new(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_response_header",
                "compiled response contains an invalid header value",
            ))
        })?;
        outgoing.headers_mut().append(name, value);
    }
    Ok(())
}

fn operation_error(
    runtime: &Runtime,
    operation: &str,
    error: simile_runtime::Error,
    head: bool,
    seed: u64,
) -> Response {
    if let Some(cause) = service_error_cause(&error) {
        if let Ok(Some(response)) =
            runtime.synthesize_protocol_error_with_seed(operation, cause, seed)
        {
            return operation_response(&response, head);
        }
    }
    runtime_error(error)
}

fn service_error_cause(error: &simile_runtime::Error) -> Option<&'static str> {
    use simile_runtime::Error as RuntimeError;

    match error {
        RuntimeError::InvalidBody { .. } => Some("invalid_body"),
        RuntimeError::MalformedJson { .. }
        | RuntimeError::MalformedBody { .. }
        | RuntimeError::InvalidParameters { .. }
        | RuntimeError::BodyRequired { .. }
        | RuntimeError::BodyNotDeclared { .. }
        | RuntimeError::EmptyReplayIdentity => Some("malformed_request"),
        RuntimeError::MissingContentType { .. }
        | RuntimeError::UnsupportedMediaType { .. }
        | RuntimeError::UnsupportedEncoding { .. } => Some("unsupported_media_type"),
        RuntimeError::IdempotencyConflict { .. } => Some("idempotency_conflict"),
        RuntimeError::AuthenticationRequired { .. } => Some("authentication_required"),
        RuntimeError::PermissionDenied { .. } => Some("permission_denied"),
        _ => None,
    }
}

fn runtime_error(error: simile_runtime::Error) -> Response {
    use simile_runtime::Error as RuntimeError;

    if let RuntimeError::InvalidParameters { issues } = &error {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "request parameters do not satisfy the operation contract",
                    "issues": issues,
                }
            }),
        );
    }

    if let RuntimeError::InvalidBody { issues } = &error {
        let message = validation_error_message(issues.len());
        return json_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({
                "error": {
                    "code": "invalid_body",
                    "message": message,
                    "issues": issues,
                }
            }),
        );
    }

    let (status, code) = match &error {
        RuntimeError::MalformedJson { .. } => (StatusCode::BAD_REQUEST, "malformed_json"),
        RuntimeError::MalformedBody { .. } => (StatusCode::BAD_REQUEST, "malformed_body"),
        RuntimeError::InvalidParameters { .. } => (StatusCode::BAD_REQUEST, "invalid_parameters"),
        RuntimeError::MissingContentType { .. }
        | RuntimeError::UnsupportedMediaType { .. }
        | RuntimeError::UnsupportedEncoding { .. } => {
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
        }
        RuntimeError::BodyRequired { .. } | RuntimeError::BodyNotDeclared { .. } => {
            (StatusCode::BAD_REQUEST, "invalid_body")
        }
        RuntimeError::IdempotencyConflict { .. } => (StatusCode::CONFLICT, "idempotency_conflict"),
        RuntimeError::AuthenticationRequired { .. } => {
            (StatusCode::UNAUTHORIZED, "authentication_required")
        }
        RuntimeError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, "permission_denied"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "emulation_failed"),
    };
    error_response(status, code, &error.to_string())
}

pub fn validation_error_message(issue_count: usize) -> String {
    if issue_count == 1 {
        "request body is invalid; fix the issue listed in `issues` and retry".to_owned()
    } else {
        format!(
            "request body is invalid; fix the {issue_count} issues listed in `issues` and retry"
        )
    }
}

fn method_not_allowed(methods: BTreeSet<&str>) -> Response {
    let mut response = error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method is not declared for this route",
    );
    let allow = methods.into_iter().collect::<Vec<_>>().join(", ");
    if let Ok(value) = HeaderValue::from_str(&allow) {
        response.headers_mut().insert(ALLOW, value);
    }
    response
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    json_response(
        status,
        json!({
            "error": {
                "code": code,
                "message": message,
            }
        }),
    )
}

fn json_response(status: StatusCode, payload: serde_json::Value) -> Response {
    let mut response = Response::new(payload.to_string().into_bytes());
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sanitizes_sensitive_incident_evidence_without_flattening_the_request() {
        let body = br#"{
          "customer": {"name": "Ada", "api_key": "sk_live_secret"},
          "password_reset_token": "reset-secret",
          "items": [{"reference": "sku_123"}]
        }"#;
        let sanitized = sanitize_body(Some("application/problem+json; charset=utf-8"), body)
            .unwrap_or_else(|| panic!("JSON incident body must be sanitizable"));
        let value: serde_json::Value = serde_json::from_slice(&sanitized)
            .unwrap_or_else(|error| panic!("sanitized body must remain JSON: {error}"));

        assert_eq!(value["customer"]["name"], "Ada");
        assert_eq!(value["customer"]["api_key"], "<redacted>");
        assert_eq!(value["password_reset_token"], "<redacted>");
        assert_eq!(value["items"][0]["reference"], "sku_123");

        let parameters = sanitize_parameters(IncomingParameters {
            path: BTreeMap::from([("access_token".to_owned(), vec!["path-secret".to_owned()])]),
            query: BTreeMap::from([("page".to_owned(), vec!["2".to_owned()])]),
            headers: BTreeMap::from([(
                "authorization".to_owned(),
                vec!["Bearer secret".to_owned()],
            )]),
            cookies: BTreeMap::from([("session_token".to_owned(), vec!["cookie".to_owned()])]),
        });
        assert_eq!(parameters.path["access_token"], ["<redacted>"]);
        assert_eq!(parameters.query["page"], ["2"]);
        assert_eq!(parameters.headers["authorization"], ["<redacted>"]);
        assert_eq!(parameters.cookies["session_token"], ["<redacted>"]);
    }

    #[test]
    fn declines_opaque_request_bodies_for_automatic_reports() {
        assert!(sanitize_body(Some("application/octet-stream"), b"secret bytes").is_none());
        assert!(sanitize_body(None, b"secret bytes").is_none());
    }
}
