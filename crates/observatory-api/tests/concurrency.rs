//! Bounded concurrency tests for the router and the rate limiter.
//!
//! The server binds an ephemeral loopback port. No public network access is
//! used. The load is deliberately small and fixed so the test is fast; the
//! assertions are exact, so a lost update in the limiter fails the test.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use observatory_api::http;
use observatory_api::Api;
use observatory_platform::{PlatformConfig, RateLimitConfig, RateLimiter};

const THREADS: usize = 8;
const REQUESTS_PER_THREAD: usize = 16;
const LIMIT: u32 = 25;

fn start_server(limit: u32) -> SocketAddr {
    let config = PlatformConfig {
        require_auth: false,
        rate_limit: RateLimitConfig {
            limit,
            window_secs: 60,
        },
        cors: observatory_platform::CorsConfig::default(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local address");
    let api = Arc::new(Api::new(config));
    std::thread::spawn(move || http::serve(listener, api));
    addr
}

struct HealthResponse {
    status: u16,
    limit: String,
    remaining: String,
    body: String,
}

fn health(addr: SocketAddr) -> HealthResponse {
    let mut stream = TcpStream::connect(addr).expect("connect to loopback server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    stream
        .write_all(b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .expect("write request");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    let raw = String::from_utf8(raw).expect("UTF-8 response");
    let (head, body) = raw.split_once("\r\n\r\n").expect("response head");
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line");

    let mut headers = std::collections::BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.to_ascii_lowercase(), value.trim().to_string());
        }
    }

    HealthResponse {
        status,
        limit: headers
            .get("x-ratelimit-limit")
            .cloned()
            .expect("rate-limit limit header"),
        remaining: headers
            .get("x-ratelimit-remaining")
            .cloned()
            .expect("rate-limit remaining header"),
        body: body.to_string(),
    }
}

#[test]
fn bounded_concurrent_load_is_stable_and_exactly_rate_limited() {
    let addr = start_server(LIMIT);
    let barrier = Arc::new(Barrier::new(THREADS));

    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                (0..REQUESTS_PER_THREAD)
                    .map(|_| health(addr))
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let responses: Vec<HealthResponse> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("worker did not panic"))
        .collect();
    assert_eq!(responses.len(), THREADS * REQUESTS_PER_THREAD);

    let allowed: Vec<&HealthResponse> = responses.iter().filter(|r| r.status == 200).collect();
    let limited = responses.iter().filter(|r| r.status == 429).count();
    assert_eq!(
        allowed.len(),
        LIMIT as usize,
        "exactly the configured limit should succeed"
    );
    assert_eq!(limited, responses.len() - LIMIT as usize);
    assert!(
        responses.iter().all(|r| r.status == 200 || r.status == 429),
        "no response may panic into a 5xx"
    );
    assert!(responses.iter().all(|r| r.limit == LIMIT.to_string()));
    assert!(responses
        .iter()
        .all(|r| r.status != 429 || r.body.contains("\"code\":\"rate_limited\"")));

    // Every allowed request consumed one distinct slot. Duplicate values would
    // mean two requests observed the same window count.
    let mut remaining: Vec<u32> = allowed
        .iter()
        .map(|r| r.remaining.parse().expect("numeric remaining"))
        .collect();
    remaining.sort_unstable();
    assert_eq!(remaining, (0..LIMIT).collect::<Vec<_>>());

    for response in allowed {
        assert!(
            response.body.contains("\"kind\":\"health\"")
                && response.body.contains("\"status\":\"ok\""),
            "allowed responses must stay identical: {}",
            response.body
        );
    }
}

#[test]
fn concurrent_limiter_checks_never_exceed_the_limit() {
    let limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        limit: LIMIT,
        window_secs: 60,
    }));
    let barrier = Arc::new(Barrier::new(THREADS));

    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                (0..REQUESTS_PER_THREAD)
                    .map(|_| limiter.check("client:test", 0))
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let decisions: Vec<_> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("worker did not panic"))
        .collect();
    assert_eq!(decisions.len(), THREADS * REQUESTS_PER_THREAD);

    let allowed: Vec<_> = decisions.iter().filter(|d| d.allowed).collect();
    assert_eq!(allowed.len(), LIMIT as usize);
    let mut remaining: Vec<u32> = allowed.iter().map(|d| d.remaining).collect();
    remaining.sort_unstable();
    assert_eq!(remaining, (0..LIMIT).collect::<Vec<_>>());
}
