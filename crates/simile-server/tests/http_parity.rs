#[path = "../../simile-http/tests/support/parity.rs"]
mod parity;

#[tokio::test]
async fn listener_matches_portable_http_bytes() -> Result<(), Box<dyn std::error::Error>> {
    use simile_service::Service;
    use simile_stack::{ClockMode, ClockSnapshot, LogicalTime};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let clock = ClockSnapshot {
        mode: ClockMode::Manual,
        now: LogicalTime::from_unix_millis(1_700_000_000_000),
    };
    let service = Service::memory_with_clock(parity::artifact()?, clock)?;
    parity::exercise(&service)?;
    let portable = Service::memory_with_clock(parity::artifact()?, clock)?;
    let routes = simile_http::RouteTable::new(portable.artifact());
    let native = Service::memory_with_clock(parity::artifact()?, clock)?;
    let server = simile_server::Server::bind_service_with_http_options(
        ([127, 0, 0, 1], 0).into(),
        native,
        parity::options(),
        None,
    )
    .await?;
    let address = server.local_addr();
    let (stop, signal) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(server.run_until(async {
        drop(signal.await);
    }));
    for (method, uri, body, expected) in parity::cases() {
        let exchange = simile_http::handle_request(
            &portable,
            &routes,
            parity::request(method, uri, body.clone())?,
            &parity::options(),
        );
        let mut socket = tokio::net::TcpStream::connect(address).await?;
        let header = format!(
            "{method} {uri} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nX-Role: reader\r\nX-Role: writer\r\nCookie: session=one\r\nCookie: session=two\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        socket.write_all(header.as_bytes()).await?;
        socket.write_all(&body).await?;
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await?;
        let split = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .ok_or("missing HTTP headers")?;
        let headers = String::from_utf8(bytes[..split].to_vec())?.to_ascii_lowercase();
        assert!(
            headers.starts_with(&format!("http/1.1 {expected}")),
            "{method} {uri}: {headers}"
        );
        for (name, value) in exchange.response.headers() {
            assert!(headers.contains(&format!("{name}: {}", value.to_str()?.to_ascii_lowercase())));
        }
        assert_eq!(
            &bytes[split + 4..],
            exchange.response.body(),
            "{method} {uri}"
        );
    }
    let _ = stop.send(());
    task.await??;
    Ok(())
}
