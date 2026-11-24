use simile_core::{session::ServiceSession, workspace};
use simile_http::{RequestBody, http};
use std::collections::BTreeMap;

fn session(location: &str) -> Result<ServiceSession, Box<dyn std::error::Error>> {
    let yaml = include_str!("../../../playground/src/templates/orders.yaml").replace(
        "in: header, name: X-API-Key",
        &format!("in: {location}, name: X-API-Key"),
    );
    let config = format!(
        "service \"orders\" {{\n listen {{\n host = \"127.0.0.1\"\n port = 4242\n }}\n openapi {{ path = \"./orders.yaml\" }}\n modules = [\"./modules/orders.hcl\"]\n}}\n{}",
        include_str!("../../../playground/src/templates/setup.hcl")
    );
    let compiled = workspace::compile_workspace(&workspace::WorkspaceRequest {
        files: BTreeMap::from([
            ("Simfile".into(), config),
            ("orders.yaml".into(), yaml),
            (
                "modules/orders.hcl".into(),
                include_str!("../../../playground/src/templates/orders.hcl").into(),
            ),
        ]),
        config_path: Some("Simfile".into()),
        contract_entry: None,
        contract_format: workspace::ContractFormat::Auto,
        modules: vec![],
        heuristic_policy: Default::default(),
    })?;
    let session = ServiceSession::new(&compiled.bytes)?;
    session.setup_profile(
        Some("application"),
        Some("test"),
        BTreeMap::from([("name".into(), serde_json::json!("integration"))]),
        true,
        7,
    )?;
    Ok(session)
}

#[test]
fn profile_credentials_preserve_unrelated_wire_fields_and_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    for location in ["header", "query", "cookie"] {
        let session = session(location)?;
        let before = session.export_checkpoint()?;
        let mut request = http::Request::builder()
            .method("GET")
            .uri("/orders?tag=a%20b&tag=c%2Fd")
            .header("x-repeat", "one")
            .header("x-repeat", "two")
            .header("cookie", "other=one")
            .header("cookie", "other=two")
            .body(RequestBody::Bytes(vec![]))?;
        simile_http::apply_profile(session.service(), "test", &mut request)?;
        assert_eq!(session.export_checkpoint()?, before);
        assert!(
            request
                .uri()
                .query()
                .ok_or("query missing")?
                .starts_with("tag=a%20b&tag=c%2Fd")
        );
        assert_eq!(request.headers().get_all("x-repeat").iter().count(), 2);
        let cookies: Vec<_> = request.headers().get_all("cookie").iter().collect();
        assert_eq!(cookies[0], "other=one");
        assert_eq!(cookies[1], "other=two");
        assert_eq!(
            session
                .handle_request_with_seed(request, Some(7))
                .response
                .status(),
            200
        );
    }
    Ok(())
}

#[test]
fn work_inspection_is_read_only_and_dead_letters_can_be_redelivered()
-> Result<(), Box<dyn std::error::Error>> {
    let mut session = session("header")?;
    let mut request = http::Request::builder()
        .method("POST")
        .uri("/orders")
        .header("content-type", "application/json")
        .body(RequestBody::Bytes(
            br#"{"product":"book","quantity":2}"#.to_vec(),
        ))?;
    simile_http::apply_profile(session.service(), "test", &mut request)?;
    assert_eq!(
        session
            .handle_request_with_seed(request, Some(7))
            .response
            .status(),
        201
    );
    let before = session.export_checkpoint()?;
    let work = session.service().inspect_work()?;
    assert_eq!(work.len(), 2);
    assert!(work.iter().all(|item| !item.leased));
    assert_eq!(session.export_checkpoint()?, before);
    for attempt in 1..=5 {
        let work = session.step_work(10)?;
        let item = work
            .iter()
            .find(|item| item.kind == "async")
            .ok_or("missing async work")?;
        assert_eq!(session.retry_work(&item.handle, 0)?, attempt < 5);
    }
    let before = session.export_checkpoint()?;
    assert_eq!(
        session
            .service()
            .inspect_work()?
            .iter()
            .filter(|item| item.dead_letter)
            .count(),
        1
    );
    assert_eq!(session.export_checkpoint()?, before);
    assert!(session.redeliver_dead_letter()?);
    assert!(
        session
            .service()
            .inspect_work()?
            .iter()
            .all(|item| !item.dead_letter)
    );
    let work = session.step_work(10)?;
    let item = work
        .iter()
        .find(|item| item.kind == "async")
        .ok_or("missing redelivery")?;
    assert_eq!(item.attempt, 1);
    session.execute_work(&item.handle)?;
    Ok(())
}
