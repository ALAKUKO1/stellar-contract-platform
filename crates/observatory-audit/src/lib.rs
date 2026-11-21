//! Bounded, deterministic contract interface and artifact heuristics.
//!
//! These checks are intentionally narrow and fully deterministic. They are
//! **not** a security audit and must not be presented as one. Heuristics marked
//! `experimental` may change or be removed.
//!
//! Each check emits a finding with a stable id so CI can suppress or track
//! individual rules.

use std::collections::BTreeSet;

use observatory_interface::ContractInterface;
use observatory_spec::{SpecSeverity, SpecType};
use observatory_wasm::{WasmReport, CONTRACT_ENV_META_SECTION, CONTRACT_META_SECTION};
use serde::{Deserialize, Serialize};

/// Overall audit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AuditStatus {
    /// No findings.
    Pass,
    /// Only warnings.
    Warn,
    /// At least one error.
    Error,
}

/// Severity of a single finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FindingSeverity {
    /// A non-fatal issue.
    Warn,
    /// A definite inconsistency.
    Error,
}

/// A single audit finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable rule id, for example `interface.empty-name`.
    pub id: String,
    /// Severity.
    pub severity: FindingSeverity,
    /// Human-readable explanation.
    pub message: String,
    /// Optional stable path.
    pub path: Option<String>,
    /// Whether the rule is experimental.
    pub experimental: bool,
}

/// The result of running heuristics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    /// Overall status.
    pub status: AuditStatus,
    /// Findings, deterministically ordered by id then path.
    pub findings: Vec<Finding>,
}

/// Every rule id this crate can emit.
///
/// `docs/security.md` documents each one; a test asserts the two stay in sync.
pub const RULE_IDS: &[&str] = &[
    "artifact.duplicate-metadata-section",
    "artifact.memory64",
    "artifact.metadata-without-code",
    "artifact.metadata-without-contract-spec",
    "artifact.no-code",
    "artifact.no-contract-spec",
    "artifact.no-memory",
    "artifact.oversized-spec",
    "artifact.truncated-report",
    "event.too-many-prefix-topics",
    "function.duplicate-param",
    "function.empty-name",
    "function.unknown-type",
    "interface.duplicate-event",
    "interface.duplicate-function",
    "interface.duplicate-type",
    "interface.empty",
    "interface.no-functions",
    "spec.error",
    "spec.info",
    "spec.warning",
    "type.duplicate-case",
    "type.duplicate-field",
];

impl AuditReport {
    fn from_findings(mut findings: Vec<Finding>) -> Self {
        findings.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.path.cmp(&b.path)));
        let status = if findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error)
        {
            AuditStatus::Error
        } else if findings.is_empty() {
            AuditStatus::Pass
        } else {
            AuditStatus::Warn
        };
        AuditReport { status, findings }
    }

    /// Merge two reports into one.
    #[must_use]
    pub fn merge(self, other: AuditReport) -> AuditReport {
        let mut findings = self.findings;
        findings.extend(other.findings);
        AuditReport::from_findings(findings)
    }

    /// Count findings of a severity.
    #[must_use]
    pub fn count(&self, severity: FindingSeverity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }
}

/// Run interface heuristics.
#[must_use]
pub fn audit_interface(interface: &ContractInterface) -> AuditReport {
    let mut findings = Vec::new();

    for issue in observatory_spec::validate(&interface.to_spec()) {
        findings.push(Finding {
            id: format!(
                "spec.{}",
                match issue.severity {
                    SpecSeverity::Error => "error",
                    SpecSeverity::Warning => "warning",
                    SpecSeverity::Info => "info",
                }
            ),
            severity: match issue.severity {
                SpecSeverity::Error => FindingSeverity::Error,
                SpecSeverity::Warning => FindingSeverity::Warn,
                SpecSeverity::Info => FindingSeverity::Warn,
            },
            message: issue.message,
            path: None,
            experimental: false,
        });
    }

    if interface.functions.is_empty() && !interface.is_empty() {
        findings.push(Finding {
            id: "interface.no-functions".to_string(),
            severity: FindingSeverity::Warn,
            message: "interface declares types or events but no functions".to_string(),
            path: None,
            experimental: true,
        });
    }

    if interface.functions.is_empty() && interface.is_empty() {
        findings.push(Finding {
            id: "interface.empty".to_string(),
            severity: FindingSeverity::Warn,
            message: "interface declares no public entities".to_string(),
            path: None,
            experimental: true,
        });
    }

    for function in &interface.functions {
        if function.name.trim().is_empty() {
            findings.push(Finding {
                id: "function.empty-name".to_string(),
                severity: FindingSeverity::Error,
                message: "a function has an empty name".to_string(),
                path: Some("function::".to_string()),
                experimental: false,
            });
        }
        let mut names: Vec<&str> = function
            .inputs
            .iter()
            .map(|input| input.name.as_str())
            .collect();
        names.sort_unstable();
        for pair in names.windows(2) {
            if !pair[0].is_empty() && pair[0] == pair[1] {
                findings.push(Finding {
                    id: "function.duplicate-param".to_string(),
                    severity: FindingSeverity::Warn,
                    message: format!(
                        "function `{}` has duplicate parameter name `{}`",
                        function.name, pair[0]
                    ),
                    path: Some(format!("function::{}", function.name)),
                    experimental: true,
                });
            }
        }
        for input in &function.inputs {
            if references_unknown(interface, &input.ty) {
                findings.push(Finding {
                    id: "function.unknown-type".to_string(),
                    severity: FindingSeverity::Error,
                    message: format!(
                        "function `{}` parameter `{}` references an unknown type",
                        function.name, input.name
                    ),
                    path: Some(format!("function::{}", function.name)),
                    experimental: false,
                });
            }
        }
    }

    for event in &interface.events {
        if event.prefix_topics.len() > 2 {
            findings.push(Finding {
                id: "event.too-many-prefix-topics".to_string(),
                severity: FindingSeverity::Error,
                message: format!(
                    "event `{}` declares {} prefix topics; the specification allows at most 2",
                    event.name,
                    event.prefix_topics.len()
                ),
                path: Some(format!("event::{}", event.name)),
                experimental: false,
            });
        }
    }

    for name in duplicate_names(interface.functions.iter().map(|item| item.name.as_str())) {
        findings.push(Finding {
            id: "interface.duplicate-function".to_string(),
            severity: FindingSeverity::Error,
            message: format!("function `{name}` is declared more than once"),
            path: Some(format!("function::{name}")),
            experimental: false,
        });
    }
    for name in duplicate_names(
        interface
            .structs
            .iter()
            .map(|item| item.name.as_str())
            .chain(interface.unions.iter().map(|item| item.name.as_str()))
            .chain(interface.enums.iter().map(|item| item.name.as_str()))
            .chain(interface.error_enums.iter().map(|item| item.name.as_str())),
    ) {
        findings.push(Finding {
            id: "interface.duplicate-type".to_string(),
            severity: FindingSeverity::Error,
            message: format!("type `{name}` is declared more than once"),
            path: Some(format!("type::{name}")),
            experimental: false,
        });
    }
    for name in duplicate_names(interface.events.iter().map(|item| item.name.as_str())) {
        findings.push(Finding {
            id: "interface.duplicate-event".to_string(),
            severity: FindingSeverity::Error,
            message: format!("event `{name}` is declared more than once"),
            path: Some(format!("event::{name}")),
            experimental: false,
        });
    }

    for item in &interface.structs {
        for name in duplicate_names(item.fields.iter().map(|field| field.name.as_str())) {
            findings.push(Finding {
                id: "type.duplicate-field".to_string(),
                severity: FindingSeverity::Warn,
                message: format!(
                    "struct `{}` declares field `{name}` more than once",
                    item.name
                ),
                path: Some(format!("type::{}", item.name)),
                experimental: true,
            });
        }
    }
    for item in &interface.unions {
        for name in duplicate_names(item.cases.iter().map(|case| case.name.as_str())) {
            findings.push(Finding {
                id: "type.duplicate-case".to_string(),
                severity: FindingSeverity::Warn,
                message: format!(
                    "union `{}` declares case `{name}` more than once",
                    item.name
                ),
                path: Some(format!("type::{}", item.name)),
                experimental: true,
            });
        }
    }
    for item in interface
        .enums
        .iter()
        .map(|item| (&item.name, &item.cases))
        .chain(
            interface
                .error_enums
                .iter()
                .map(|item| (&item.name, &item.cases)),
        )
    {
        for name in duplicate_names(item.1.iter().map(|case| case.name.as_str())) {
            findings.push(Finding {
                id: "type.duplicate-case".to_string(),
                severity: FindingSeverity::Warn,
                message: format!("enum `{}` declares case `{name}` more than once", item.0),
                path: Some(format!("type::{}", item.0)),
                experimental: true,
            });
        }
    }

    AuditReport::from_findings(findings)
}

/// Run artifact heuristics.
#[must_use]
pub fn audit_artifact(report: &WasmReport) -> AuditReport {
    let mut findings = Vec::new();

    if report.code_section_bytes == 0 {
        findings.push(Finding {
            id: "artifact.no-code".to_string(),
            severity: FindingSeverity::Warn,
            message: "module contains no code section".to_string(),
            path: None,
            experimental: false,
        });
    }
    if !report.has_contract_spec {
        findings.push(Finding {
            id: "artifact.no-contract-spec".to_string(),
            severity: FindingSeverity::Warn,
            message: "module has no `contractspecv0` section; it may not be a Soroban contract"
                .to_string(),
            path: None,
            experimental: true,
        });
    }
    if report.truncated {
        findings.push(Finding {
            id: "artifact.truncated-report".to_string(),
            severity: FindingSeverity::Warn,
            message: "import, export, or custom-section listing was truncated by limits"
                .to_string(),
            path: None,
            experimental: false,
        });
    }
    if report.memories.is_empty() {
        findings.push(Finding {
            id: "artifact.no-memory".to_string(),
            severity: FindingSeverity::Warn,
            message: "module declares no linear memory".to_string(),
            path: None,
            experimental: true,
        });
    }
    if report.memories.iter().any(|memory| memory.memory64) {
        findings.push(Finding {
            id: "artifact.memory64".to_string(),
            severity: FindingSeverity::Warn,
            message: "module declares a 64-bit memory (memory64 proposal); Soroban \
                      contracts use 32-bit linear memory"
                .to_string(),
            path: None,
            experimental: true,
        });
    }
    if let Some(spec_bytes) = report.contract_spec_bytes {
        if spec_bytes > 4 * 1024 * 1024 {
            findings.push(Finding {
                id: "artifact.oversized-spec".to_string(),
                severity: FindingSeverity::Warn,
                message: format!(
                    "contract specification section is unusually large ({spec_bytes} bytes)"
                ),
                path: None,
                experimental: true,
            });
        }
    }

    let metadata_sections: Vec<&str> = report
        .custom_sections
        .iter()
        .filter(|section| {
            section.name == CONTRACT_META_SECTION || section.name == CONTRACT_ENV_META_SECTION
        })
        .map(|section| section.name.as_str())
        .collect();
    if !metadata_sections.is_empty() && !report.has_contract_spec {
        findings.push(Finding {
            id: "artifact.metadata-without-contract-spec".to_string(),
            severity: FindingSeverity::Warn,
            message: "module carries Soroban metadata but no `contractspecv0` section".to_string(),
            path: None,
            experimental: true,
        });
    }
    if !metadata_sections.is_empty() && report.code_section_bytes == 0 {
        findings.push(Finding {
            id: "artifact.metadata-without-code".to_string(),
            severity: FindingSeverity::Warn,
            message: "module carries Soroban metadata but no code section".to_string(),
            path: None,
            experimental: true,
        });
    }
    let mut reported_sections = BTreeSet::new();
    let mut seen_sections = BTreeSet::new();
    for name in metadata_sections {
        if !seen_sections.insert(name) && reported_sections.insert(name) {
            findings.push(Finding {
                id: "artifact.duplicate-metadata-section".to_string(),
                severity: FindingSeverity::Warn,
                message: format!("custom section `{name}` is declared more than once"),
                path: Some(name.to_string()),
                experimental: true,
            });
        }
    }

    AuditReport::from_findings(findings)
}

fn duplicate_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for name in names {
        if !name.is_empty() && !seen.insert(name) {
            duplicates.insert(name);
        }
    }
    duplicates.into_iter().collect()
}

fn references_unknown(interface: &ContractInterface, ty: &SpecType) -> bool {
    match ty {
        SpecType::Udt { name } => !interface.has_type(name),
        SpecType::Option(inner) | SpecType::Vec(inner) => references_unknown(interface, inner),
        SpecType::Result { ok, error } => {
            references_unknown(interface, ok) || references_unknown(interface, error)
        }
        SpecType::Map { key, value } => {
            references_unknown(interface, key) || references_unknown(interface, value)
        }
        SpecType::Tuple(types) => types.iter().any(|ty| references_unknown(interface, ty)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_spec::normalize;
    use observatory_testutil::spec::{
        spec_enum, spec_error_enum, spec_event, spec_function, spec_struct, spec_union, ty_udt,
    };
    use observatory_testutil::{
        custom_section, encode_spec_entries, minimal_module, module_with_spec,
        CONTRACT_SPEC_SECTION,
    };
    use stellar_xdr::{ScSpecEntry, ScSpecEventParamLocationV0, ScSpecTypeDef};

    fn interface(entries: Vec<ScSpecEntry>) -> ContractInterface {
        ContractInterface::from_spec(normalize(entries))
    }

    /// Well-formed interfaces that must stay free of audit findings.
    fn interface_corpus() -> Vec<(&'static str, Vec<ScSpecEntry>)> {
        vec![
            (
                "token",
                vec![
                    spec_function(
                        "transfer",
                        &[("to", ty_udt("Recipient")), ("amount", ScSpecTypeDef::I128)],
                        Some(ScSpecTypeDef::Bool),
                    ),
                    spec_function(
                        "balance",
                        &[("owner", ScSpecTypeDef::Address)],
                        Some(ScSpecTypeDef::I128),
                    ),
                    spec_struct("Recipient", &[("address", ScSpecTypeDef::Address)]),
                    spec_enum("Status", &[("Active", 0), ("Frozen", 1)]),
                    spec_union(
                        "Change",
                        &[("None", None), ("Amount", Some(vec![ScSpecTypeDef::I128]))],
                    ),
                    spec_error_enum("Error", &[("NotAuthorized", 1)]),
                    spec_event(
                        "Transfer",
                        &["transfer"],
                        &[(
                            "amount",
                            ScSpecTypeDef::I128,
                            ScSpecEventParamLocationV0::Data,
                        )],
                    ),
                ],
            ),
            (
                "greeting",
                vec![
                    spec_function(
                        "hello",
                        &[("name", ScSpecTypeDef::String)],
                        Some(ScSpecTypeDef::Symbol),
                    ),
                    spec_struct("Greeting", &[("name", ScSpecTypeDef::String)]),
                ],
            ),
            (
                "counter",
                vec![
                    spec_function("increment", &[], None),
                    spec_function("get", &[], Some(ScSpecTypeDef::U32)),
                ],
            ),
        ]
    }

    /// A valid module with a code section, linear memory, and a contract spec.
    fn module_with_code_and_spec(entries: &[ScSpecEntry]) -> Vec<u8> {
        let mut module =
            wat::parse_str(r#"(module (memory 1) (func (export "hello")))"#).expect("valid wat");
        module.extend(custom_section(
            CONTRACT_SPEC_SECTION,
            &encode_spec_entries(entries),
        ));
        module
    }

    #[test]
    fn clean_interface_passes() {
        let report = audit_interface(&interface(vec![
            spec_function("hello", &[("to", ty_udt("Greeting"))], None),
            spec_struct("Greeting", &[("name", ScSpecTypeDef::String)]),
        ]));
        assert_eq!(report.status, AuditStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn duplicate_functions_are_errors() {
        let report = audit_interface(&interface(vec![
            spec_function("a", &[], None),
            spec_function("a", &[], None),
        ]));
        assert_eq!(report.status, AuditStatus::Error);
        assert!(report.count(FindingSeverity::Error) >= 1);
    }

    #[test]
    fn unknown_type_reference_is_error() {
        let report = audit_interface(&interface(vec![spec_function(
            "f",
            &[("x", ty_udt("Missing"))],
            None,
        )]));
        assert_eq!(report.status, AuditStatus::Error);
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "function.unknown-type"));
    }

    fn module_with_metadata(entries: &[ScSpecEntry], sections: &[&str]) -> Vec<u8> {
        let mut module = module_with_code_and_spec(entries);
        for name in sections {
            module.extend(custom_section(name, &[]));
        }
        module
    }

    #[test]
    fn duplicate_interface_entries_are_errors() {
        let report = audit_interface(&interface(vec![
            spec_function("a", &[], None),
            spec_function("a", &[], None),
            spec_struct("S", &[]),
            spec_enum("S", &[("A", 0)]),
            spec_event("evt", &[], &[]),
            spec_event("evt", &[], &[]),
        ]));
        for id in [
            "interface.duplicate-function",
            "interface.duplicate-type",
            "interface.duplicate-event",
        ] {
            assert!(
                report.findings.iter().any(|finding| finding.id == id),
                "missing {id}: {:?}",
                report.findings
            );
        }
        assert_eq!(report.status, AuditStatus::Error);
    }

    #[test]
    fn duplicate_interface_entries_are_not_invented() {
        let report = audit_interface(&interface(vec![
            spec_function("a", &[], None),
            spec_struct("S", &[]),
            spec_enum("E", &[("A", 0)]),
            spec_event("evt", &[], &[]),
        ]));
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.id.starts_with("interface.duplicate")));
    }

    #[test]
    fn duplicate_members_are_warnings() {
        let report = audit_interface(&interface(vec![
            spec_struct("S", &[("a", ScSpecTypeDef::U32), ("a", ScSpecTypeDef::U32)]),
            spec_union("U", &[("A", None), ("A", Some(vec![ScSpecTypeDef::U32]))]),
            spec_enum("E", &[("A", 0), ("A", 1)]),
        ]));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.id == "type.duplicate-field"));
        assert!(
            report
                .findings
                .iter()
                .filter(|finding| finding.id == "type.duplicate-case")
                .count()
                >= 2
        );
    }

    #[test]
    fn duplicate_members_are_not_invented() {
        let report = audit_interface(&interface(vec![
            spec_struct("S", &[("a", ScSpecTypeDef::U32), ("b", ScSpecTypeDef::U32)]),
            spec_union("U", &[("A", None), ("B", None)]),
            spec_enum("E", &[("A", 0), ("B", 1)]),
        ]));
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.id == "type.duplicate-field"
                || finding.id == "type.duplicate-case"));
    }

    #[test]
    fn artifact_metadata_without_spec_is_reported() {
        let mut module =
            wat::parse_str(r#"(module (memory 1) (func (export "hello")))"#).expect("valid wat");
        module.extend(custom_section(observatory_wasm::CONTRACT_META_SECTION, &[]));
        let report = audit_artifact(&observatory_wasm::inspect(&module).unwrap());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.id == "artifact.metadata-without-contract-spec"));

        let clean = module_with_metadata(
            &[spec_function("hello", &[], None)],
            &[CONTRACT_META_SECTION],
        );
        let report = audit_artifact(&observatory_wasm::inspect(&clean).unwrap());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.id == "artifact.metadata-without-contract-spec"));
    }

    #[test]
    fn artifact_metadata_without_code_is_reported() {
        let mut module = module_with_spec(&[spec_function("hello", &[], None)]);
        module.extend(custom_section(CONTRACT_META_SECTION, &[]));
        let report = audit_artifact(&observatory_wasm::inspect(&module).unwrap());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.id == "artifact.metadata-without-code"));

        let clean = module_with_metadata(
            &[spec_function("hello", &[], None)],
            &[CONTRACT_META_SECTION],
        );
        let report = audit_artifact(&observatory_wasm::inspect(&clean).unwrap());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.id == "artifact.metadata-without-code"));
    }

    #[test]
    fn duplicate_metadata_sections_are_reported_once() {
        let module = module_with_metadata(
            &[spec_function("hello", &[], None)],
            &[CONTRACT_META_SECTION, CONTRACT_META_SECTION],
        );
        let report = audit_artifact(&observatory_wasm::inspect(&module).unwrap());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.id == "artifact.duplicate-metadata-section")
                .count(),
            1
        );

        let clean = module_with_metadata(
            &[spec_function("hello", &[], None)],
            &[CONTRACT_META_SECTION],
        );
        let report = audit_artifact(&observatory_wasm::inspect(&clean).unwrap());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.id == "artifact.duplicate-metadata-section"));
    }

    #[test]
    fn artifact_without_spec_warns() {
        let wasm = module_with_spec(&[spec_function("f", &[], None)]);
        let inspected = observatory_wasm::inspect(&wasm).unwrap();
        let report = audit_artifact(&inspected);
        assert_eq!(report.status, AuditStatus::Warn);
        assert!(!report
            .findings
            .iter()
            .any(|f| f.id == "artifact.no-contract-spec"));
        let empty = observatory_wasm::inspect(&minimal_module()).unwrap();
        let report = audit_artifact(&empty);
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "artifact.no-contract-spec"));
    }

    #[test]
    fn false_positive_corpus_is_clean() {
        for (name, entries) in interface_corpus() {
            let report = audit_interface(&interface(entries));
            assert!(
                report.status == AuditStatus::Pass && report.findings.is_empty(),
                "false positive in interface fixture `{name}`: {:?}",
                report.findings
            );
        }
        for (name, entries) in interface_corpus() {
            let wasm = module_with_code_and_spec(&entries);
            let inspected = observatory_wasm::inspect(&wasm).unwrap();
            let report = audit_artifact(&inspected);
            assert!(
                report.status == AuditStatus::Pass && report.findings.is_empty(),
                "false positive in artifact fixture `{name}`: {:?}",
                report.findings
            );
        }
    }

    #[test]
    fn merge_is_deterministic() {
        let a = audit_interface(&interface(vec![spec_function("a", &[], None)]));
        let b = audit_artifact(&observatory_wasm::inspect(&minimal_module()).unwrap());
        let first = a.clone().merge(b.clone());
        let second = a.merge(b);
        assert_eq!(first, second);
    }

    #[test]
    fn registered_rule_ids_are_sorted_and_unique() {
        let mut sorted = RULE_IDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, RULE_IDS);
    }

    #[test]
    fn every_registered_rule_is_documented() {
        let doc = include_str!("../../../docs/security.md");
        for id in RULE_IDS {
            assert!(
                doc.contains(&format!("`{id}`")),
                "rule `{id}` is missing from docs/security.md"
            );
        }
    }

    #[test]
    fn emitted_rule_ids_are_registered() {
        let report = audit_interface(&interface(vec![
            spec_function(
                "",
                &[("x", ty_udt("Missing")), ("x", ScSpecTypeDef::U32)],
                None,
            ),
            spec_function("a", &[], None),
            spec_function("a", &[], None),
            spec_struct("S", &[("f", ScSpecTypeDef::U32), ("f", ScSpecTypeDef::U32)]),
            spec_enum("E", &[("A", 0), ("A", 1)]),
        ]));
        let artifact = audit_artifact(&observatory_wasm::inspect(&minimal_module()).unwrap());
        for finding in report.findings.iter().chain(&artifact.findings) {
            assert!(
                RULE_IDS.contains(&finding.id.as_str()),
                "unregistered rule id `{}`",
                finding.id
            );
        }
    }
}
