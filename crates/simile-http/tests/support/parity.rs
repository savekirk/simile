use simile_http::{HttpOptions, RequestBody, http};
use simile_service::Service;

pub fn artifact() -> Result<simile_artifact::Artifact, Box<dyn std::error::Error>> {
    let compiled = simile_compiler::Compiler::new().compile(simile_compiler::CompileRequest {
        openapi: simile_compiler::Source::new(
            "http-parity.yaml",
            include_str!("../fixtures/parity.yaml"),
        ),
        modules: vec![],
        heuristic_policy: Default::default(),
    })?;
    Ok(simile_artifact::decode(&simile_artifact::encode(
        &compiled,
    )?)?)
}

pub fn options() -> HttpOptions {
    HttpOptions {
        response_seed: Some(u64::MAX),
        body_limit: 64,
        ..Default::default()
    }
}

pub fn cases() -> Vec<(&'static str, &'static str, Vec<u8>, u16)> {
    vec![
        ("GET", "/items/7?tag=one&tag=two", vec![], 200),
        ("GET", "/items/7?tag=one", vec![], 400),
        ("GET", "/items/no?tag=one&tag=two", vec![], 400),
        ("HEAD", "/items/7", vec![], 200),
        ("PUT", "/items/7", vec![], 405),
        ("HEAD", "/absent", vec![], 404),
        ("POST", "/items", br#"{"name":"fixture"}"#.to_vec(), 201),
        ("POST", "/items", b"{".to_vec(), 400),
        ("POST", "/items", vec![b' '; 65], 413),
    ]
}

pub fn request(
    method: &str,
    uri: &str,
    body: Vec<u8>,
) -> Result<http::Request<RequestBody>, http::Error> {
    http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-role", "reader")
        .header("x-role", "writer")
        .header("cookie", "session=one")
        .header("cookie", "session=two")
        .body(RequestBody::Bytes(body))
}

pub fn exercise(service: &Service) -> Result<(), Box<dyn std::error::Error>> {
    let routes = simile_http::RouteTable::new(service.artifact());
    for (method, uri, body, expected) in cases() {
        let exchange =
            simile_http::handle_request(service, &routes, request(method, uri, body)?, &options());
        assert_eq!(
            exchange.response.status().as_u16(),
            expected,
            "{method} {uri}"
        );
        if method == "HEAD" {
            assert!(exchange.response.body().is_empty());
        }
    }
    let signature = simile_spec::WebhookSigning {
        header: "x-signature".to_owned(),
        timestamp_header: "x-timestamp".to_owned(),
        secret: "secret".to_owned(),
    };
    let prepared = simile_http::prepare_webhook(
        Some(&signature),
        "POST",
        Some("application/json"),
        "https://example.test/hook",
        &serde_json::json!({"id":"ord_42"}),
        1_700_000_000,
    )?;
    assert_eq!(prepared.body(), br#"{"id":"ord_42"}"#);
    assert_eq!(prepared.headers()["x-timestamp"], "1700000000");
    assert_eq!(
        prepared.headers()["x-signature"],
        "t=1700000000,v1=fbbea5de96730ac365c91a8731570f69ffed16e1d727eb80ae028a3dbab8aebb"
    );
    Ok(())
}
