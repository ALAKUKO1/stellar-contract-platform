//! Golden-file tests for every JSON report kind.
//!
//! Each report is generated from deterministic fixtures, normalised for the
//! volatile envelope version (and any absolute path or timestamp that may leak
//! into a report), then compared byte-for-byte against a checked-in golden.
//!
//! Regenerate the goldens after an intentional output change with:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p observatory-cli --test golden
//! ```
//!
//! (On PowerShell: `$env:UPDATE_GOLDEN=1; cargo test -p observatory-cli --test golden`.)

use std::io::Write;
use std::path::PathBuf;

use assert_cmd::Command;
use observatory_testutil::module_with_spec;
use observatory_testutil::spec::{spec_function, spec_struct, ty_udt};
use serde_json::{json, Value};
use stellar_xdr::{ScSpecEntry, ScSpecTypeDef};

fn binary() -> Command {
    Command::cargo_bin("stellar-contract-platform").expect("binary builds")
}

fn write_temp(bytes: &[u8], suffix: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    file.write_all(bytes).expect("write temp file");
    file.flush().expect("flush temp file");
    file
}

fn contract_entries() -> Vec<ScSpecEntry> {
    vec![
        spec_function(
            "transfer",
            &[("to", ty_udt("Recipient")), ("amount", ScSpecTypeDef::I128)],
            Some(ScSpecTypeDef::Bool),
        ),
        spec_struct("Recipient", &[("address", ScSpecTypeDef::Address)]),
    ]
}

fn golden_path(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{kind}.json"))
}

fn looks_like_absolute_path(value: &str) -> bool {
    value.starts_with('/')
        || (value.len() > 2 && value.as_bytes()[1] == b':' && value.as_bytes()[2] == b'\\')
}

/// Strip values that legitimately differ between machines or runs.
fn normalize(value: &mut Value, top_level: bool) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if top_level && key == "version" && child.is_string() {
                    *child = json!("<version>");
                } else if key.contains("timestamp") {
                    *child = json!("<timestamp>");
                } else {
                    normalize(child, false);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize(item, false);
            }
        }
        Value::String(text) if looks_like_absolute_path(text) => *text = "<path>".to_string(),
        _ => {}
    }
}

fn assert_golden(kind: &str, expected_code: i32, args: &[String]) {
    let output = binary().args(args).output().expect("run binary");
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "`{kind}` exited unexpectedly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut value: Value = serde_json::from_slice(&output.stdout).expect("report is valid JSON");
    normalize(&mut value, true);
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("serialize report")
    );

    let path = golden_path(kind);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create golden directory");
        }
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    assert_eq!(
        actual,
        expected,
        "`{kind}` report differs from {}; regenerate with UPDATE_GOLDEN=1",
        path.display()
    );
}

fn string_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

#[test]
fn inspect_report_matches_golden() {
    let file = write_temp(&module_with_spec(&contract_entries()), ".wasm");
    assert_golden(
        "inspect",
        0,
        &string_args(&["inspect", file.path().to_str().unwrap(), "--json"]),
    );
}

#[test]
fn spec_report_matches_golden() {
    let file = write_temp(&module_with_spec(&contract_entries()), ".wasm");
    assert_golden(
        "spec",
        0,
        &string_args(&["spec", file.path().to_str().unwrap(), "--json"]),
    );
}

#[test]
fn events_report_matches_golden() {
    use stellar_xdr::{Limits, ScSymbol, StringM, WriteXdr};
    let topic = stellar_xdr::ScVal::Symbol(ScSymbol(StringM::try_from("transfer").unwrap()))
        .to_xdr_base64(Limits::none())
        .unwrap();
    let fixture = format!(
        r#"{{"events":[{{"contractId":"CAAA","ledger":10,"type":"contract","topics":["{topic}"],"data":null}}]}}"#
    );
    let file = write_temp(fixture.as_bytes(), ".json");
    assert_golden(
        "events",
        0,
        &string_args(&["events", file.path().to_str().unwrap(), "--json"]),
    );
}

#[test]
fn diff_report_matches_golden() {
    let old = write_temp(
        &module_with_spec(&[spec_function("a", &[], None), spec_function("b", &[], None)]),
        ".wasm",
    );
    let new = write_temp(&module_with_spec(&[spec_function("a", &[], None)]), ".wasm");
    assert_golden(
        "diff",
        0,
        &string_args(&[
            "diff",
            old.path().to_str().unwrap(),
            new.path().to_str().unwrap(),
            "--json",
        ]),
    );
}

#[test]
fn compat_report_matches_golden() {
    let old = write_temp(
        &module_with_spec(&[spec_function("a", &[], None), spec_function("b", &[], None)]),
        ".wasm",
    );
    let new = write_temp(&module_with_spec(&[spec_function("a", &[], None)]), ".wasm");
    assert_golden(
        "compat",
        6,
        &string_args(&[
            "compat",
            old.path().to_str().unwrap(),
            new.path().to_str().unwrap(),
            "--json",
        ]),
    );
}

#[test]
fn fingerprint_report_matches_golden() {
    let file = write_temp(&module_with_spec(&contract_entries()), ".wasm");
    assert_golden(
        "fingerprint",
        0,
        &string_args(&["fingerprint", file.path().to_str().unwrap(), "--json"]),
    );
}

fn instance_xdr(hash: [u8; 32]) -> String {
    use stellar_xdr::{
        ContractDataDurability, ContractDataEntry, ContractExecutable, ContractId, ExtensionPoint,
        Hash, LedgerEntryData, Limits, ScAddress, ScContractInstance, ScVal, WriteXdr,
    };
    let data = LedgerEntryData::ContractData(ContractDataEntry {
        ext: ExtensionPoint::V0,
        contract: ScAddress::Contract(ContractId(Hash([9u8; 32]))),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
        val: ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::Wasm(Hash(hash)),
            storage: None,
        }),
    });
    data.to_xdr_base64(Limits::none()).unwrap()
}

fn instance_fixture(hash: [u8; 32]) -> String {
    format!(
        r#"{{"getLedgerEntries":{{"__sequence":[{{"entries":[{{"key":"","xdr":"{}","lastModifiedLedgerSeq":123,"liveUntilLedgerSeq":456}}]}}]}}}}"#,
        instance_xdr(hash)
    )
}

#[test]
fn deployment_report_matches_golden() {
    let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
    let fixture = write_temp(instance_fixture([7u8; 32]).as_bytes(), ".json");
    assert_golden(
        "deployment",
        0,
        &string_args(&[
            "deployment",
            "inspect",
            &contract_id,
            "--rpc-fixture",
            fixture.path().to_str().unwrap(),
            "--json",
        ]),
    );
}

#[test]
fn verify_report_matches_golden() {
    let wasm = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
    let wasm_file = write_temp(&wasm, ".wasm");
    let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
    let hash = observatory_wasm::artifact_fingerprint(&wasm);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hex::decode(hash).unwrap());
    let fixture = write_temp(instance_fixture(bytes).as_bytes(), ".json");
    assert_golden(
        "verify",
        0,
        &string_args(&[
            "verify",
            "artifact",
            wasm_file.path().to_str().unwrap(),
            "--contract",
            &contract_id,
            "--rpc-fixture",
            fixture.path().to_str().unwrap(),
            "--json",
        ]),
    );
}
