//! Live transport tests against a loopback mock server. No external network.
#![cfg(feature = "live")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use observatory_rpc::{Endpoint, HttpTransport, RpcClient, RpcErrorKind, Transport};

fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read request");
        if read == 0 {
            return buffer;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let expected = header_end + 4 + content_length;
        while buffer.len() < expected {
            let read = stream.read(&mut chunk).expect("read body");
            if read == 0 {
                return buffer;
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        return buffer;
    }
}

fn spawn_server(body: String) -> (String, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("local address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let request = read_request(&mut stream);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        request
    });
    (format!("http://{address}"), handle)
}

#[test]
fn fetches_a_json_rpc_response_over_loopback() {
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"sequence":42,"id":"ledger-42"}}"#;
    let (url, handle) = spawn_server(body.to_string());
    let endpoint = Endpoint::parse(&url).expect("loopback endpoint");
    let transport = HttpTransport::new(endpoint.clone()).expect("transport");
    let client = RpcClient::new(endpoint, transport);
    let ledger = client.get_latest_ledger().expect("latest ledger");
    assert_eq!(ledger.sequence, 42);
    assert_eq!(ledger.id.as_deref(), Some("ledger-42"));
    handle.join().expect("server joined");
}

#[test]
fn bounds_the_response_size() {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"sequence":1,"pad":"{}"}}}}"#,
        "x".repeat(4096)
    );
    let (url, handle) = spawn_server(body);
    let endpoint = Endpoint::parse(&url).expect("loopback endpoint");
    let transport =
        HttpTransport::with_limits(endpoint, Duration::from_secs(5), 128).expect("transport");
    let error = transport
        .send(&serde_json::json!({ "jsonrpc": "2.0", "id": 1 }))
        .expect_err("response must exceed the limit");
    assert!(error.to_string().contains("128 byte limit"), "{error}");
    handle.join().expect("server joined");
}

#[test]
fn posts_json_rpc_to_the_mock_server_and_normalizes_errors() {
    let body = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}"#;
    let (url, handle) = spawn_server(body.to_string());
    let endpoint = Endpoint::parse(&url).expect("loopback endpoint");
    let transport = HttpTransport::new(endpoint.clone()).expect("transport");
    let client = RpcClient::new(endpoint, transport);

    let error = client.get_network().expect_err("server error envelope");
    assert_eq!(
        error.rpc_error().unwrap().kind(),
        RpcErrorKind::MethodNotFound
    );

    let request =
        String::from_utf8_lossy(&handle.join().expect("server joined")).to_ascii_lowercase();
    assert!(request.starts_with("post / http/1.1"), "{request}");
    assert!(
        request.contains("content-type: application/json"),
        "{request}"
    );
    assert!(request.contains("\"method\":\"getnetwork\""), "{request}");
}
