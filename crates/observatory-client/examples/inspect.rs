//! Inspect a contract through a running Observatory API.
//!
//! ```bash
//! cargo run -p observatory-client --example inspect -- \
//!   http://127.0.0.1:8080 sco_... contract.wasm
//! ```

use std::time::Duration;

use observatory_client::{Client, Policy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let base_url = args
        .next()
        .ok_or("usage: inspect <BASE_URL> <API_KEY> <WASM>")?;
    let api_key = args
        .next()
        .ok_or("usage: inspect <BASE_URL> <API_KEY> <WASM>")?;
    let wasm_path = args
        .next()
        .ok_or("usage: inspect <BASE_URL> <API_KEY> <WASM>")?;

    let client = Client::builder(base_url)
        .api_key(api_key)
        .timeout(Duration::from_secs(30))
        .build()?;
    let wasm = std::fs::read(wasm_path)?;

    let report = client.inspect(&wasm, true)?;
    println!("artifact: {}", report.wasm.artifact_sha256);
    println!("exports: {}", report.wasm.exports.len());
    println!(
        "contract spec: {}",
        if report.wasm.has_contract_spec {
            "present"
        } else {
            "absent"
        }
    );
    if let Some(audit) = report.audit {
        println!(
            "audit: {:?} ({} findings)",
            audit.status,
            audit.findings.len()
        );
    }

    let fingerprint = client.fingerprint(&wasm)?;
    println!("interface: {:?}", fingerprint.interface_sha256);
    println!(
        "compatibility with itself: {:?}",
        client.compatibility(&wasm, &wasm, Policy::Strict)?.status
    );
    Ok(())
}
