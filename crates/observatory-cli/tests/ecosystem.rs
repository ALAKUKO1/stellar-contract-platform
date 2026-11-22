//! Interoperability tests against checked-in, Soroban-built artifacts.
//!
//! The artifacts and their build provenance live in `fixtures/soroban/`. The
//! assertions cover stable summaries (names, counts, shapes), never artifact
//! hashes or other values that change per build.

use std::collections::BTreeSet;

use observatory_interface::{ContractInterface, INTERFACE_SCHEMA_VERSION};
use observatory_spec::SpecType;
use observatory_wasm::WasmReport;

const ASSET_TOKEN: &[u8] = include_bytes!("../../../fixtures/soroban/asset-token/asset-token.wasm");
const COUNTER: &[u8] = include_bytes!("../../../fixtures/soroban/counter/counter.wasm");

fn function_names(interface: &ContractInterface) -> Vec<&str> {
    interface
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect()
}

fn exported_functions(report: &WasmReport) -> BTreeSet<&str> {
    report
        .exports
        .iter()
        .filter(|export| export.kind == "func")
        .map(|export| export.name.as_str())
        .collect()
}

#[test]
fn inspects_a_real_soroban_token_artifact() {
    let report = observatory_wasm::inspect(ASSET_TOKEN).expect("valid WASM");
    assert_eq!(report.version, 1);
    assert!(report.has_contract_spec);
    assert!(report.contract_spec_bytes.unwrap_or_default() > 0);

    let exports = exported_functions(&report);
    for expected in ["initialize", "transfer", "balance", "metadata"] {
        assert!(exports.contains(expected), "missing export `{expected}`");
    }
}

#[test]
fn extracts_the_token_specification() {
    let spec = observatory_spec::parse_wasm(ASSET_TOKEN).expect("contract spec");
    let summary = observatory_spec::summarize(&spec);
    assert_eq!(summary.functions, 6);
    assert_eq!(summary.structs, 1);
    assert_eq!(summary.enums, 1);
    assert_eq!(summary.error_enums, 1);
    assert_eq!(summary.events, 0);
    assert!(summary.issues.is_empty());

    let names: BTreeSet<&str> = spec.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        BTreeSet::from([
            "admin",
            "balance",
            "initialize",
            "metadata",
            "set_status",
            "transfer"
        ])
    );

    let transfer = spec.function("transfer").expect("transfer");
    assert_eq!(
        transfer
            .inputs
            .iter()
            .map(|input| input.name.as_str())
            .collect::<Vec<_>>(),
        ["_from", "_to", "_amount"]
    );
    assert_eq!(
        transfer.outputs,
        [SpecType::Result {
            ok: Box::new(SpecType::Tuple(Vec::new())),
            error: Box::new(SpecType::Udt {
                name: "TokenError".to_string()
            }),
        }]
    );

    let metadata = spec
        .structs
        .iter()
        .find(|item| item.name == "TokenMetadata");
    let metadata = metadata.expect("TokenMetadata");
    assert_eq!(
        metadata
            .fields
            .iter()
            .map(|field| (field.name.as_str(), &field.ty))
            .collect::<Vec<_>>(),
        [
            ("decimals", &SpecType::U32),
            ("name", &SpecType::Symbol),
            ("symbol", &SpecType::Symbol),
        ]
    );
}

#[test]
fn builds_a_canonical_interface_from_a_real_artifact() {
    let interface = ContractInterface::from_wasm(ASSET_TOKEN).expect("canonical interface");
    assert_eq!(interface.schema_version, INTERFACE_SCHEMA_VERSION);
    assert_eq!(
        function_names(&interface),
        [
            "admin",
            "balance",
            "initialize",
            "metadata",
            "set_status",
            "transfer"
        ]
    );
    assert!(interface.is_subset_of(&interface));
    // Fingerprinting is covered by deterministic vectors in
    // observatory-interface; the value is intentionally not asserted here.
    assert!(!interface.fingerprint().expect("fingerprint").is_empty());
}

#[test]
fn inspects_and_extracts_a_second_real_artifact() {
    let report = observatory_wasm::inspect(COUNTER).expect("valid WASM");
    assert!(report.has_contract_spec);
    let exports = exported_functions(&report);
    for expected in ["increment", "value", "state"] {
        assert!(exports.contains(expected), "missing export `{expected}`");
    }

    let spec = observatory_spec::parse_wasm(COUNTER).expect("contract spec");
    let summary = observatory_spec::summarize(&spec);
    assert_eq!(summary.functions, 3);
    assert_eq!(summary.structs, 1);
    assert_eq!(summary.enums, 0);
    assert_eq!(summary.error_enums, 0);
    assert!(summary.issues.is_empty());

    let interface = ContractInterface::from_wasm(COUNTER).expect("canonical interface");
    assert_eq!(function_names(&interface), ["increment", "state", "value"]);
    let state = interface.function("state").expect("state");
    assert_eq!(
        state.outputs,
        [SpecType::Udt {
            name: "CounterState".to_string()
        }]
    );
}
