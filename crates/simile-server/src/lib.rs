#![forbid(unsafe_code)]

mod delivery;
mod process;
mod proxy;

pub use process::CommandResult;
pub use proxy::{ProxyOptions, ProxyServer};

use std::fmt;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, Bytes, HttpBody, to_bytes};
use axum::extract::{ConnectInfo, State};
use axum::http::header::{CONTENT_TYPE, USER_AGENT};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use futures_util::stream;
use simile_artifact::Artifact;
use simile_http::RouteTable;
use simile_runtime::TransportPlan;
use simile_service::Service;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not bind service to {address}: {source}")]
    Bind {
        address: SocketAddr,
        #[source]
        source: std::io::Error,
    },

    #[error("could not inspect the bound service address: {0}")]
    LocalAddress(#[source] std::io::Error),

    #[error("service listener failed: {0}")]
    Serve(#[source] std::io::Error),

    #[error("compiled service has no scenario named `{0}`")]
    UnknownScenario(String),

    #[error("compiled service has no fault profile named `{0}`")]
    UnknownFaultProfile(String),

    #[error("could not initialize fault profile `{profile}`: {message}")]
    FaultProfileState { profile: String, message: String },

    #[error("could not initialize service state: {0}")]
    Service(#[from] simile_service::Error),

    #[error("a test command is required")]
    CommandMissing,

    #[error("could not start test command `{command}`")]
    CommandSpawn {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("could not wait for the test command")]
    CommandWait(#[source] std::io::Error),

    #[error("could not listen for test interruption")]
    CommandSignal(#[source] std::io::Error),

    #[error("the service stopped before the test command completed")]
    CommandServiceStopped,

    #[error("the supervised service task failed")]
    CommandServiceTask(#[source] tokio::task::JoinError),
}

#[derive(Clone, Debug, Default)]
pub struct ServerOptions {
    pub scenario: Option<String>,
    pub fault_profile: Option<String>,
    pub incident_sender: Option<mpsc::Sender<RuntimeIncident>>,
}

pub use simile_http::RuntimeIncident;
pub struct Server {
    listener: TcpListener,
    router: Router,
    address: SocketAddr,
    fault_seed: Option<u64>,
    service: Arc<Service>,
}

struct ServerState {
    service: Arc<Service>,
    routes: RouteTable,
    body_limit: usize,
    response_seed: Option<u64>,
    scenario: Option<String>,
    fault_profile: Option<String>,
    incident_sender: Option<mpsc::Sender<RuntimeIncident>>,
}

struct AccessLine<'a> {
    method: &'a Method,
    path: &'a str,
    status: StatusCode,
    origin_ip: Option<IpAddr>,
    user_agent: Option<&'a str>,
    request_bytes: Option<u64>,
    request_content_type: Option<&'a str>,
    response_bytes: Option<u64>,
    response_content_type: Option<&'a str>,
    elapsed: Duration,
}

impl Server {
    pub async fn bind(address: SocketAddr, artifact: Artifact) -> Result<Self> {
        Self::bind_with_options(address, artifact, ServerOptions::default()).await
    }

    pub async fn bind_with_scenario(
        address: SocketAddr,
        artifact: Artifact,
        scenario: Option<String>,
    ) -> Result<Self> {
        Self::bind_with_options(
            address,
            artifact,
            ServerOptions {
                scenario,
                fault_profile: None,
                incident_sender: None,
            },
        )
        .await
    }

    pub async fn bind_with_options(
        address: SocketAddr,
        artifact: Artifact,
        options: ServerOptions,
    ) -> Result<Self> {
        let service = Service::memory(artifact)?;
        Self::bind_service_with_options(address, service, options).await
    }

    /// Bind a service and persist its complete Stack at `state_path`.
    pub async fn bind_durable_with_options(
        address: SocketAddr,
        artifact: Artifact,
        state_path: impl AsRef<Path>,
        options: ServerOptions,
    ) -> Result<Self> {
        let service = Service::durable(artifact, state_path)?;
        Self::bind_service_with_options(address, service, options).await
    }

    /// Bind a service whose state is owned by the supplied supervisor.
    pub async fn bind_service_with_options(
        address: SocketAddr,
        service: Service,
        options: ServerOptions,
    ) -> Result<Self> {
        Self::bind_service_with_http_options(
            address,
            service,
            simile_http::HttpOptions {
                scenario: options.scenario,
                fault_profile: options.fault_profile,
                ..Default::default()
            },
            options.incident_sender,
        )
        .await
    }

    /// Bind with explicit HTTP controls for reproducible embeddings and parity fixtures.
    pub async fn bind_service_with_http_options(
        address: SocketAddr,
        service: Service,
        options: simile_http::HttpOptions,
        incident_sender: Option<mpsc::Sender<RuntimeIncident>>,
    ) -> Result<Self> {
        let artifact = service.artifact();
        if let Some(name) = options.scenario.as_deref() {
            if !artifact.has_scenario(name) {
                return Err(Error::UnknownScenario(name.to_owned()));
            }
        }
        if let Some(name) = options.fault_profile.as_deref() {
            if !artifact.has_fault_profile(name) {
                return Err(Error::UnknownFaultProfile(name.to_owned()));
            }
        }
        let listener = TcpListener::bind(address)
            .await
            .map_err(|source| Error::Bind { address, source })?;
        let address = listener.local_addr().map_err(Error::LocalAddress)?;
        let routes = RouteTable::new(artifact);
        let fault_seed = options
            .fault_profile
            .as_deref()
            .map(|profile| {
                service
                    .runtime()
                    .fault_profile_seed(profile)
                    .map_err(|error| Error::FaultProfileState {
                        profile: profile.to_owned(),
                        message: error.to_string(),
                    })
            })
            .transpose()?;
        let service = Arc::new(service);
        let state = Arc::new(ServerState {
            service: Arc::clone(&service),
            routes,
            body_limit: options.body_limit,
            response_seed: options.response_seed,
            scenario: options.scenario,
            fault_profile: options.fault_profile,
            incident_sender,
        });
        let router = Router::new()
            .fallback(handle)
            .layer(middleware::from_fn(access_log))
            .with_state(state);
        Ok(Self {
            listener,
            router,
            address,
            fault_seed,
            service,
        })
    }

    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub const fn fault_seed(&self) -> Option<u64> {
        self.fault_seed
    }

    pub async fn run(self) -> Result<()> {
        self.run_until(async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::warn!(%error, "could not install Ctrl-C handler");
            }
        })
        .await
    }

    pub async fn run_until<F>(self, shutdown: F) -> Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let dispatcher = tokio::spawn(delivery::run(Arc::clone(&self.service)));
        let result = axum::serve(
            self.listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(Error::Serve);
        dispatcher.abort();
        result
    }
}

impl fmt::Display for AccessLine<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} {} origin=",
            self.method,
            self.path,
            self.status.as_u16()
        )?;
        write_ip(formatter, self.origin_ip)?;
        formatter.write_str(" user-agent=")?;
        write_label(formatter, self.user_agent)?;
        formatter.write_str(" request=")?;
        write_size(formatter, self.request_bytes)?;
        formatter.write_str(" request-type=")?;
        write_label(formatter, self.request_content_type)?;
        formatter.write_str(" response=")?;
        write_size(formatter, self.response_bytes)?;
        formatter.write_str(" response-type=")?;
        write_label(formatter, self.response_content_type)?;
        write!(formatter, " elapsed={}us", self.elapsed.as_micros())
    }
}

fn write_ip(formatter: &mut fmt::Formatter<'_>, address: Option<IpAddr>) -> fmt::Result {
    match address {
        Some(address) => write!(formatter, "{address}"),
        None => formatter.write_str("-"),
    }
}

fn write_size(formatter: &mut fmt::Formatter<'_>, bytes: Option<u64>) -> fmt::Result {
    match bytes {
        Some(bytes) => write!(formatter, "{bytes}B"),
        None => formatter.write_str("?"),
    }
}

fn write_label(formatter: &mut fmt::Formatter<'_>, value: Option<&str>) -> fmt::Result {
    match value {
        Some(value) => write!(formatter, "{value:?}"),
        None => formatter.write_str("-"),
    }
}

async fn access_log(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let origin_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connection| connection.0.ip());
    let user_agent = text_header(request.headers(), USER_AGENT.as_str());
    let request_bytes = request.body().size_hint().exact();
    let request_content_type = text_header(request.headers(), CONTENT_TYPE.as_str());
    let started = Instant::now();
    let response = next.run(request).await;
    let response_content_type = text_header(response.headers(), CONTENT_TYPE.as_str());
    let line = AccessLine {
        method: &method,
        path: &path,
        status: response.status(),
        origin_ip,
        user_agent: user_agent.as_deref(),
        request_bytes,
        request_content_type: request_content_type.as_deref(),
        response_bytes: response.body().size_hint().exact(),
        response_content_type: response_content_type.as_deref(),
        elapsed: started.elapsed(),
    };
    let origin = line.origin_ip.map(|value| value.to_string());
    tracing::info!(
        target: "simile::http",
        method = %line.method,
        path = line.path,
        status = line.status.as_u16(),
        origin = origin.as_deref().unwrap_or("-"),
        user_agent = line.user_agent.unwrap_or("-"),
        request_bytes = line.request_bytes,
        request_type = line.request_content_type.unwrap_or("-"),
        response_bytes = line.response_bytes,
        response_type = line.response_content_type.unwrap_or("-"),
        elapsed_us = line.elapsed.as_micros(),
        "request completed"
    );
    response
}

fn text_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn handle(State(state): State<Arc<ServerState>>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, state.body_limit).await {
        Ok(bytes) => simile_http::RequestBody::Bytes(bytes.to_vec()),
        Err(_) => simile_http::RequestBody::TooLarge,
    };
    let mut exchange = simile_http::handle_request(
        &state.service,
        &state.routes,
        Request::from_parts(parts, body),
        &simile_http::HttpOptions {
            body_limit: state.body_limit,
            scenario: state.scenario.clone(),
            fault_profile: state.fault_profile.clone(),
            response_seed: state.response_seed,
        },
    );
    if let Some(incident) = exchange.incident.take() {
        if let Some(sender) = &state.incident_sender {
            if let Err(error) = sender.try_send(incident) {
                tracing::debug!(target: "simile::reporting", %error, "automatic report skipped because the reporting queue is unavailable");
            }
        }
    }
    if let Some(fault) = &exchange.fault {
        tracing::info!(target: "simile::fault", operation = exchange.operation, fault = fault.name(), source = ?fault.source(), phase = ?fault.phase(), "activated environmental fault");
    }
    send_exchange(exchange).await
}

async fn send_exchange(exchange: simile_http::HttpExchange) -> Response {
    let (parts, bytes) = exchange.response.into_parts();
    let body = match exchange.transport {
        TransportPlan::Respond => Body::from(bytes),
        TransportPlan::RespondAfter { milliseconds } => {
            tokio::time::sleep(Duration::from_millis(milliseconds)).await;
            Body::from(bytes)
        }
        TransportPlan::DisconnectAfterCommit => {
            let error = std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "simile injected a disconnect after commit",
            );
            Body::from_stream(stream::once(
                async move { Err::<Bytes, std::io::Error>(error) },
            ))
        }
    };
    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use simile_compiler::{CompileRequest, Compiler, Source};
    use simile_heuristics::Policy;
    use simile_http::{Match, validation_error_message};
    use std::collections::BTreeSet;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    fn artifact() -> Artifact {
        let source = r#"
openapi: 3.1.0
info: { title: Orders, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: createOrder
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              additionalProperties: false
              required: [customer, items]
              properties:
                customer:
                  type: object
                  required: [email]
                  properties:
                    email: { type: string }
                items:
                  type: array
                  minItems: 1
                  items:
                    type: object
                    required: [quantity]
                    properties:
                      quantity: { type: integer, minimum: 1 }
      responses:
        '201':
          description: created
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
        '400':
          description: invalid order request
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [code, message]
                properties:
                  code: { const: invalid_order }
                  message: { const: order request rejected }
  /orders/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: { type: string, pattern: '^ord_' }
    get:
      operationId: getOrder
      parameters:
        - name: limit
          in: query
          schema: { type: integer, minimum: 1 }
        - name: X-Trace
          in: header
          schema: { type: string, minLength: 3 }
      responses:
        '200':
          description: found
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
  /orders/recent:
    get:
      operationId: recentOrders
      responses:
        '204': { description: empty }
  /file-links/{id}/open:
    get:
      operationId: openFileLink
      responses:
        '303':
          description: open file
          headers:
            Location:
              schema: { type: string, format: uri }
              example: https://files.example.test/download/file_1234
"#;
        compile_artifact(source)
    }

    fn artifact_without_client_error() -> Artifact {
        compile_artifact(
            r#"
openapi: 3.1.0
info: { title: Orders, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: createOrder
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [customer]
              properties:
                customer: { type: string }
      responses:
        '201':
          description: created
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
"#,
        )
    }

    fn artifact_with_faults() -> Artifact {
        compile_artifact_with_behavior(
            r#"
openapi: 3.1.0
info: { title: Orders, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: createOrder
      responses:
        '201':
          description: created
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
"#,
            r#"
fault "slow_response" {
  phase = "before_response"
  delay { milliseconds = 20 }
}

fault "response_lost" {
  phase = "after_commit"
  disconnect {}
}

fault_profile "always_slow" {
  probability       = 1
  faults            = ["slow_response"]
  operations        = ["createOrder"]
  max_consecutive   = 1
  cooldown_requests = 1
  seed              = 7
}

scenario "slow" {
  inject "createOrder" { fault = "slow_response" }
}

scenario "lost" {
  inject "createOrder" { fault = "response_lost" }
}
"#,
        )
    }

    fn artifact_with_authentication() -> Artifact {
        compile_artifact_with_behavior(
            r#"
openapi: 3.1.0
info: { title: Secured orders, version: 1.0.0 }
paths:
  /private:
    get:
      operationId: privateOrders
      security:
        - merchantKey: []
      responses:
        '200':
          description: allowed
          content:
            application/json:
              schema:
                type: object
                required: [allowed]
                properties:
                  allowed: { const: true }
components:
  securitySchemes:
    merchantKey:
      type: apiKey
      in: header
      name: X-Merchant-Key
"#,
            r#"
principal "merchant" {
  credential "merchantKey" {}
}

profile "merchant" {
  principal = principal.merchant
  credential "merchantKey" { value = "sim_test_merchant" }
}
"#,
        )
    }

    fn compile_artifact(source: &str) -> Artifact {
        compile_artifact_with_behavior(source, "")
    }

    fn compile_artifact_with_behavior(source: &str, behavior: &str) -> Artifact {
        let compiled = Compiler::new()
            .compile(CompileRequest {
                openapi: Source::new("orders.yml", source),
                modules: (!behavior.is_empty())
                    .then(|| Source::new("faults.hcl", behavior))
                    .into_iter()
                    .collect(),
                heuristic_policy: Policy::Standard,
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let encoded = simile_artifact::encode(&compiled).unwrap_or_else(|error| panic!("{error}"));
        simile_artifact::decode(&encoded).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn formats_a_single_safe_access_log_line() {
        let method = Method::POST;
        let line = AccessLine {
            method: &method,
            path: "/orders",
            status: StatusCode::CREATED,
            origin_ip: Some(IpAddr::from([127, 0, 0, 1])),
            user_agent: Some("simile-test/1.0"),
            request_bytes: Some(47),
            request_content_type: Some("application/json"),
            response_bytes: Some(58),
            response_content_type: Some("application/json"),
            elapsed: Duration::from_micros(321),
        };

        assert_eq!(
            line.to_string(),
            "POST /orders 201 origin=127.0.0.1 user-agent=\"simile-test/1.0\" request=47B \
             request-type=\"application/json\" response=58B response-type=\"application/json\" \
             elapsed=321us"
        );
    }

    #[test]
    fn formats_validation_error_summaries() {
        assert_eq!(
            validation_error_message(1),
            "request body is invalid; fix the issue listed in `issues` and retry"
        );
        assert_eq!(
            validation_error_message(3),
            "request body is invalid; fix the 3 issues listed in `issues` and retry"
        );
    }

    #[test]
    fn prefers_literal_routes_and_reports_allowed_methods() {
        let artifact = artifact();
        let routes = RouteTable::new(&artifact);

        assert!(matches!(
            routes.find(&Method::GET, "/orders/recent"),
            Match::Operation { operation: "GET /orders/recent", path } if path.is_empty()
        ));
        assert!(matches!(
            routes.find(&Method::GET, "/orders/ord_123"),
            Match::Operation { operation: "GET /orders/{id}", path }
                if path.get("id") == Some(&vec!["ord_123".to_owned()])
        ));
        assert!(matches!(
            routes.find(&Method::POST, "/orders/ord_123"),
            Match::MethodNotAllowed(methods) if methods == BTreeSet::from(["GET"])
        ));
        assert!(matches!(
            routes.find(&Method::GET, "/customers/cus_123"),
            Match::NotFound
        ));
    }

    #[tokio::test]
    async fn binds_to_an_ephemeral_loopback_port() {
        let server = Server::bind(SocketAddr::from(([127, 0, 0, 1], 0)), artifact())
            .await
            .unwrap_or_else(|error| panic!("{error}"));

        assert_ne!(server.local_addr().port(), 0);
        assert!(server.local_addr().ip().is_loopback());
    }

    #[tokio::test]
    async fn rejects_an_unknown_scenario_before_binding() {
        let result = Server::bind_with_scenario(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            artifact(),
            Some("missing".to_owned()),
        )
        .await;

        assert!(matches!(result, Err(Error::UnknownScenario(name)) if name == "missing"));
    }

    #[tokio::test]
    async fn rejects_an_unknown_fault_profile_before_binding() {
        let result = Server::bind_with_options(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            artifact_with_faults(),
            ServerOptions {
                scenario: None,
                fault_profile: Some("missing".to_owned()),
                incident_sender: None,
            },
        )
        .await;

        assert!(matches!(result, Err(Error::UnknownFaultProfile(name)) if name == "missing"));
    }

    #[tokio::test]
    async fn reports_the_active_fault_profiles_seed() {
        let server = Server::bind_with_options(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            artifact_with_faults(),
            ServerOptions {
                scenario: None,
                fault_profile: Some("always_slow".to_owned()),
                incident_sender: None,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(server.fault_seed(), Some(7));
    }

    #[tokio::test]
    async fn enacts_delay_and_disconnect_transport_plans() {
        let service =
            Service::memory(artifact_with_faults()).unwrap_or_else(|error| panic!("{error}"));
        let routes = RouteTable::new(service.artifact());
        for scenario in ["slow", "lost"] {
            let request = Request::builder()
                .method("POST")
                .uri("/orders")
                .body(simile_http::RequestBody::Bytes(Vec::new()))
                .unwrap_or_else(|error| panic!("{error}"));
            let exchange = simile_http::handle_request(
                &service,
                &routes,
                request,
                &simile_http::HttpOptions {
                    scenario: Some(scenario.to_owned()),
                    ..Default::default()
                },
            );
            let started = Instant::now();
            let response = send_exchange(exchange).await;
            assert_eq!(response.status(), StatusCode::CREATED);
            if scenario == "slow" {
                assert!(started.elapsed() >= Duration::from_millis(10));
            } else {
                assert!(to_bytes(response.into_body(), 1_024).await.is_err());
            }
        }
    }

    async fn serve_raw_with_artifact(artifact: Artifact, request: &[u8]) -> String {
        let server = Server::bind(SocketAddr::from(([127, 0, 0, 1], 0)), artifact)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let address = server.local_addr();
        let (shutdown, signal) = oneshot::channel::<()>();
        let task = tokio::spawn(server.run_until(async {
            drop(signal.await);
        }));
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        stream
            .write_all(request)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let _ = shutdown.send(());
        task.await
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|error| panic!("{error}"));
        String::from_utf8(response).unwrap_or_else(|error| panic!("{error}"))
    }

    async fn serve_raw(request: &[u8]) -> String {
        serve_raw_with_artifact(artifact(), request).await
    }

    #[tokio::test]
    async fn serves_a_request_through_the_operation_runtime() {
        let response = serve_raw(
            b"GET /orders/ord_123 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("content-type: application/json"));
        assert!(response.contains(r#""id":"#));
    }

    #[tokio::test]
    async fn serves_declared_redirect_headers() {
        let response = serve_raw(
            b"GET /file-links/link_123/open HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 303 See Other\r\n"));
        assert!(response.contains("location: https://files.example.test/download/file_1234\r\n"));
    }

    #[tokio::test]
    async fn enforces_declared_authentication_before_operation_execution() {
        let missing = serve_raw_with_artifact(
            artifact_with_authentication(),
            b"GET /private HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(missing.starts_with("HTTP/1.1 401 Unauthorized\r\n"));
        assert!(missing.contains(r#""code":"authentication_required""#));

        let accepted = serve_raw_with_artifact(
            artifact_with_authentication(),
            b"GET /private HTTP/1.1\r\nHost: localhost\r\nX-Merchant-Key: sim_test_merchant\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(accepted.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(accepted.contains(r#""allowed":true"#));
    }

    #[tokio::test]
    async fn validates_path_query_and_header_parameters_before_execution() {
        let invalid = serve_raw(
            b"GET /orders/not-an-order?limit=0 HTTP/1.1\r\nHost: localhost\r\nX-Trace: ok\r\nConnection: close\r\n\r\n",
        )
        .await;
        let (_, body) = invalid
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("response should contain a body"));
        let payload: serde_json::Value =
            serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}"));

        assert!(invalid.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert_eq!(payload["error"]["code"], "invalid_parameters");
        assert_eq!(payload["error"]["issues"].as_array().map(Vec::len), Some(3));

        let valid = serve_raw(
            b"GET /orders/ord_123?limit=2 HTTP/1.1\r\nHost: localhost\r\nX-Trace: trace-1\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(valid.starts_with("HTTP/1.1 200 OK\r\n"));
    }

    #[tokio::test]
    async fn returns_the_service_declared_validation_error() {
        let body = br#"{"customer":{},"items":[{"quantity":0}],"debug":true}"#;
        let request = format!(
            "POST /orders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        );
        let response = serve_raw(request.as_bytes()).await;
        let (_, body) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("response should contain headers and a body"));
        let payload: serde_json::Value =
            serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}"));

        assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert_eq!(
            payload,
            json!({
                "code": "invalid_order",
                "message": "order request rejected"
            })
        );
    }

    #[tokio::test]
    async fn falls_back_to_simile_diagnostics_without_a_declared_client_error() {
        let body = br#"{}"#;
        let request = format!(
            "POST /orders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        );
        let response =
            serve_raw_with_artifact(artifact_without_client_error(), request.as_bytes()).await;
        let (_, body) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("response should contain headers and a body"));
        let payload: serde_json::Value =
            serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}"));

        assert!(response.starts_with("HTTP/1.1 422 Unprocessable Entity\r\n"));
        assert_eq!(payload["error"]["code"], "invalid_body");
        assert_eq!(
            payload["error"]["issues"],
            json!([{
                "path": "/customer",
                "rule": "required",
                "message": "required property `customer` is missing"
            }])
        );
    }
}
