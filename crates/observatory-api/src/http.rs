//! A minimal, bounded HTTP/1.1 transport.
//!
//! The API is transport-agnostic; this module is a small, dependency-free
//! adapter suitable for local and test use. It bounds request sizes and reads
//! exactly `Content-Length` bytes. A production-grade server framework can be
//! added later without changing the router or services.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::api::Api;

/// Maximum accepted request body size (8 MiB; base64 payloads are smaller).
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of header lines.
pub const MAX_HEADERS: usize = 100;
/// Default maximum accepted request line size.
pub const DEFAULT_MAX_REQUEST_LINE_BYTES: usize = 8 * 1024;
/// Default maximum accepted single header line size.
pub const DEFAULT_MAX_HEADER_LINE_BYTES: usize = 8 * 1024;
/// Default socket read timeout.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Default socket write timeout.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Default maximum number of concurrent connections.
pub const DEFAULT_MAX_CONNECTIONS: usize = 64;

/// Bounds that protect the adapter from slow, oversized, or abusive clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpLimits {
    /// Maximum concurrent in-flight connections; excess connections get 503.
    pub max_connections: usize,
    /// Socket read timeout.
    pub read_timeout: Duration,
    /// Socket write timeout.
    pub write_timeout: Duration,
    /// Maximum request line size in bytes.
    pub max_request_line_bytes: usize,
    /// Maximum single header line size in bytes.
    pub max_header_line_bytes: usize,
    /// Maximum number of header lines.
    pub max_headers: usize,
    /// Maximum request body size in bytes.
    pub max_body_bytes: usize,
}

impl Default for HttpLimits {
    fn default() -> Self {
        HttpLimits {
            max_connections: DEFAULT_MAX_CONNECTIONS,
            read_timeout: DEFAULT_READ_TIMEOUT,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            max_request_line_bytes: DEFAULT_MAX_REQUEST_LINE_BYTES,
            max_header_line_bytes: DEFAULT_MAX_HEADER_LINE_BYTES,
            max_headers: MAX_HEADERS,
            max_body_bytes: MAX_BODY_BYTES,
        }
    }
}

/// A parsed HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// HTTP method, uppercased.
    pub method: String,
    /// Request path, without query string.
    pub path: String,
    /// Query string, if any.
    pub query: Option<String>,
    /// Lowercased header names to values.
    pub headers: BTreeMap<String, String>,
    /// Raw body bytes.
    pub body: Vec<u8>,
}

impl Request {
    /// Read a header by lowercase name.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// An HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
}

impl Response {
    /// Build an empty response.
    #[must_use]
    pub fn empty(status: u16) -> Self {
        Response {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Build a JSON response.
    #[must_use]
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        Response {
            status,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body,
        }
    }

    /// Build a text response with an explicit content type.
    #[must_use]
    pub fn text(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        Response {
            status,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body: body.into().into_bytes(),
        }
    }

    /// Add a header.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    /// Serialize to wire bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = reason_phrase(self.status);
        let mut out = format!("HTTP/1.1 {} {}\r\n", self.status, reason).into_bytes();
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        out.extend_from_slice(b"Connection: close\r\n\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

/// Parse an HTTP request from a stream using the default limits.
pub fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    read_request_with_limits(stream, &HttpLimits::default())
}

/// Parse an HTTP request from a stream with explicit limits.
///
/// Oversized request lines, header lines, header counts, and bodies are rejected
/// with [`std::io::ErrorKind::InvalidData`]. A socket timeout surfaces as the
/// matching I/O error so slow clients cannot hold a worker indefinitely.
pub fn read_request_with_limits(
    stream: &mut TcpStream,
    limits: &HttpLimits,
) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if read_line_limited(
        &mut reader,
        limits.max_request_line_bytes,
        &mut request_line,
    )? == 0
    {
        return Ok(None);
    }
    let request_line = request_line.trim_end_matches(['\r', '\n']);
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let target = parts.next().unwrap_or_default().to_string();
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target, None),
    };

    let mut headers = BTreeMap::new();
    let mut count = 0;
    loop {
        let mut line = String::new();
        if read_line_limited(&mut reader, limits.max_header_line_bytes, &mut line)? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        count += 1;
        if count > limits.max_headers {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "too many headers",
            ));
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if length > limits.max_body_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    Ok(Some(Request {
        method,
        path,
        query,
        headers,
        body,
    }))
}

fn read_line_limited<R: BufRead>(
    reader: &mut R,
    max: usize,
    out: &mut String,
) -> std::io::Result<usize> {
    let mut buffer = Vec::new();
    let read = reader.take(max as u64 + 1).read_until(b'\n', &mut buffer)?;
    if read > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "line exceeds the configured limit",
        ));
    }
    *out = String::from_utf8(buffer).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "line is not valid UTF-8")
    })?;
    Ok(read)
}

/// Serve requests until the listener is dropped or an accept error occurs.
///
/// Uses [`HttpLimits::default`]. Socket timeouts, a bounded connection count,
/// and bounded request sizes protect the adapter from slow or oversized clients.
pub fn serve(listener: TcpListener, api: Arc<Api>) {
    serve_with_limits(listener, api, HttpLimits::default());
}

/// Serve requests with explicit limits.
pub fn serve_with_limits(listener: TcpListener, api: Arc<Api>, limits: HttpLimits) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if active.load(Ordering::SeqCst) >= limits.max_connections {
            let _ = stream.write_all(
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let api = Arc::clone(&api);
        let active = Arc::clone(&active);
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(limits.read_timeout));
            let _ = stream.set_write_timeout(Some(limits.write_timeout));
            let client_id = stream
                .peer_addr()
                .map(|addr| addr.ip().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            let now = now_secs();
            if let Ok(Some(request)) = read_request_with_limits(&mut stream, &limits) {
                let response = api.handle(&request, &client_id, now);
                let _ = stream.write_all(&response.to_bytes());
                let _ = stream.flush();
            }
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Current wall-clock time in seconds.
#[must_use]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serializes_with_content_length() {
        let response = Response::json(200, &serde_json::json!({ "ok": true }));
        let bytes = response.to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: application/json"));
        assert!(text.contains("Content-Length: "));
        assert!(text.contains("{\"ok\":true}"));
    }

    #[test]
    fn reads_a_simple_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"POST /api/v1/health HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\n{}")
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let request = read_request(&mut server).unwrap().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/v1/health");
        assert_eq!(request.body, b"{}");
    }

    #[test]
    fn rejects_oversized_request_line() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let long_path = "a".repeat(256);
        client
            .write_all(format!("GET /{long_path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let limits = HttpLimits {
            max_request_line_bytes: 64,
            ..HttpLimits::default()
        };
        let error = read_request_with_limits(&mut server, &limits).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_oversized_header_line() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let long_value = "a".repeat(256);
        client
            .write_all(
                format!("GET / HTTP/1.1\r\nHost: x\r\nX-Long: {long_value}\r\n\r\n").as_bytes(),
            )
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let limits = HttpLimits {
            max_header_line_bytes: 32,
            ..HttpLimits::default()
        };
        let error = read_request_with_limits(&mut server, &limits).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_too_many_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\nA: 1\r\nB: 2\r\nC: 3\r\n\r\n")
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let limits = HttpLimits {
            max_headers: 2,
            ..HttpLimits::default()
        };
        let error = read_request_with_limits(&mut server, &limits).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("too many headers"));
    }

    #[test]
    fn read_timeout_rejects_silent_clients() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = TcpStream::connect(addr).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let error = read_request_with_limits(&mut server, &HttpLimits::default()).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
    }
}
