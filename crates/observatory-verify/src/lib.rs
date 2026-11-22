//! Local-versus-deployed verification and reproducibility metadata analysis.
//!
//! # What verification means here
//!
//! Verification compares the **artifact identity** of a local WASM file against
//! the deployed contract's executable hash. It does **not** reproduce the build
//! from source and therefore does not claim source-level reproducibility. Build
//! metadata parsing is provided to reason about reproducibility separately.
//!
//! Metadata is read from the real Soroban custom sections:
//!
//! * `contractenvmetav0` — `SCEnvMetaEntry` with protocol and pre-release.
//! * `contractmetav0` — `SCMetaEntry` key/value pairs.

use std::collections::BTreeMap;
use std::io::Cursor;

use observatory_compat::{assess, CompatibilityAssessment, CompatibilityPolicy};
use observatory_core::limits::MAX_SPEC_ENTRIES;
use observatory_core::{ObservatoryError, Result};
use observatory_diff::{diff, InterfaceDiff};
use observatory_interface::ContractInterface;
use observatory_rpc::{RpcClient, Transport};
use serde::{Deserialize, Serialize};
use stellar_xdr::{Limited, Limits, ReadXdr, ScEnvMetaEntry, ScMetaEntry, ScMetaV0};

/// The outcome of a local-versus-deployed comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStatus {
    /// The local artifact matches the deployed executable hash.
    Match,
    /// The local artifact does not match the deployed executable hash.
    Mismatch,
    /// There was not enough on-chain data to decide.
    InsufficientData,
    /// The comparison could not be completed.
    Error,
}

/// The result of verifying a local artifact against a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationResult {
    /// The verdict.
    pub status: VerificationStatus,
    /// The contract id that was queried.
    pub contract_id: String,
    /// The network label used.
    pub network: String,
    /// SHA-256 of the local artifact.
    pub local_wasm_hash: String,
    /// The deployed executable hash, if known.
    pub deployed_wasm_hash: Option<String>,
    /// A human-readable explanation.
    pub detail: String,
}

/// Environment metadata embedded in `contractenvmetav0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EnvMeta {
    /// Protocol version the contract targets.
    pub protocol: u32,
    /// Pre-release version.
    pub pre_release: u32,
}

/// Build metadata embedded in `contractmetav0`.
///
/// Known keys are exposed through typed accessors; the raw map is preserved so
/// unknown keys survive comparison and round-tripping.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BuildMetadata {
    /// Metadata key/value pairs.
    pub entries: BTreeMap<String, String>,
}

impl BuildMetadata {
    /// Compiler version key (`rsver`).
    pub const COMPILER_KEY: &'static str = "rsver";
    /// SDK version key (`rssdkver`).
    pub const SDK_KEY: &'static str = "rssdkver";
    /// Build target key (`target`).
    pub const TARGET_KEY: &'static str = "target";
    /// Build profile key (`profile`).
    pub const PROFILE_KEY: &'static str = "profile";
    /// Dependency key prefix (`dep:<name>`).
    pub const DEPENDENCY_PREFIX: &'static str = "dep:";

    /// Look up a metadata value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// The compiler version (`rsver`), when recorded.
    #[must_use]
    pub fn compiler(&self) -> Option<&str> {
        self.get(Self::COMPILER_KEY)
    }

    /// The Soroban SDK version (`rssdkver`), when recorded.
    #[must_use]
    pub fn sdk(&self) -> Option<&str> {
        self.get(Self::SDK_KEY)
    }

    /// The build target (`target`), when recorded.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.get(Self::TARGET_KEY)
    }

    /// The build profile (`profile`), when recorded.
    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.get(Self::PROFILE_KEY)
    }

    /// Dependency entries recorded as `dep:<name>` keys, sorted by name.
    ///
    /// Returns `None` when no such key is present: absence of dependency data is
    /// reported as unavailable, never as an empty dependency list. The `dep:`
    /// namespace is an Observatory convention, not an upstream Stellar standard;
    /// keys outside it are left in `entries` verbatim and are never interpreted
    /// as dependencies.
    #[must_use]
    pub fn dependencies(&self) -> Option<Vec<DependencyEntry>> {
        let mut dependencies: Vec<DependencyEntry> = self
            .entries
            .iter()
            .filter_map(|(key, value)| {
                let name = key.strip_prefix(Self::DEPENDENCY_PREFIX)?;
                Some(DependencyEntry {
                    name: name.to_string(),
                    version: (!value.is_empty()).then(|| value.clone()),
                })
            })
            .collect();
        if dependencies.is_empty() {
            return None;
        }
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        Some(dependencies)
    }

    /// Parse a metadata document from JSON (`{ "key": "value", ... }`).
    pub fn from_json(value: &serde_json::Value) -> Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| ObservatoryError::invalid("build metadata must be a JSON object"))?;
        let mut entries = BTreeMap::new();
        for (key, value) in object {
            let value = value.as_str().ok_or_else(|| {
                ObservatoryError::invalid(format!("metadata `{key}` is not a string"))
            })?;
            entries.insert(key.clone(), value.to_string());
        }
        Ok(BuildMetadata { entries })
    }
}

/// A dependency recorded in build metadata under a `dep:<name>` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyEntry {
    /// Dependency name taken from the key, verbatim.
    pub name: String,
    /// Version string taken from the value, or `null` when the value was empty.
    pub version: Option<String>,
}

/// The outcome of comparing dependency metadata across two artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyComparisonState {
    /// At least one artifact has no dependency metadata to compare.
    Unavailable,
    /// Both artifacts record identical dependency entries.
    Match,
    /// The recorded dependency entries differ.
    Mismatch,
}

/// A single differing dependency entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyDifference {
    /// Dependency name.
    pub name: String,
    /// Version on the left, if present.
    pub left: Option<String>,
    /// Version on the right, if present.
    pub right: Option<String>,
}

/// The comparison of dependency metadata from two builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyComparison {
    /// Comparison state.
    pub state: DependencyComparisonState,
    /// Number of dependencies compared (union of both name sets).
    pub compared_dependencies: usize,
    /// Differences, sorted by dependency name.
    pub differences: Vec<DependencyDifference>,
    /// Human-readable explanation.
    pub detail: String,
}

/// Dependency metadata for an artifact and its comparison with a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyReport {
    /// `dep:` entries from the artifact, or `null` when none were recorded.
    pub dependencies: Option<Vec<DependencyEntry>>,
    /// Comparison with a reference artifact, if one was supplied.
    pub comparison: DependencyComparison,
}

/// A single differing metadata key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetadataDifference {
    /// The key.
    pub key: String,
    /// Value on the left, if present.
    pub left: Option<String>,
    /// Value on the right, if present.
    pub right: Option<String>,
}

/// The comparison of two build metadata documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetadataComparison {
    /// Whether the two documents are identical.
    pub equal: bool,
    /// Number of keys compared (union of both key sets).
    pub compared_keys: usize,
    /// Differences, sorted by key.
    pub differences: Vec<MetadataDifference>,
}

/// Whether embedded metadata was compared against a reference artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataComparisonState {
    /// No reference artifact was supplied.
    NotCompared,
    /// Embedded environment and build metadata match the reference.
    Match,
    /// At least one embedded metadata value differs from the reference.
    Mismatch,
}

/// The comparison portion of a reproducibility report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReproducibilityComparison {
    /// Comparison state.
    pub state: MetadataComparisonState,
    /// Whether `contractenvmetav0` matches, when a reference was supplied.
    pub env_meta_equal: Option<bool>,
    /// Build metadata comparison, when both artifacts carry it.
    pub build_metadata: Option<MetadataComparison>,
    /// Human-readable explanation.
    pub detail: String,
}

/// What is known about a build artifact's reproducibility.
///
/// This report never claims source reproducibility: no build is performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReproducibilityReport {
    /// Parsed `contractenvmetav0`, if present.
    pub env_meta: Option<EnvMeta>,
    /// Parsed `contractmetav0`, if present.
    pub build_metadata: Option<BuildMetadata>,
    /// Comparison against a reference artifact, if one was supplied.
    pub comparison: ReproducibilityComparison,
    /// Always `not_performed`; source is never rebuilt.
    pub source_reproduction: String,
}

/// Build a reproducibility report from embedded metadata.
///
/// When `reference` is supplied, both artifacts' metadata is compared. The
/// result describes metadata equality only, never source-level reproducibility.
pub fn reproducibility_report(
    wasm: &[u8],
    reference: Option<&[u8]>,
) -> Result<ReproducibilityReport> {
    let env_meta = parse_env_meta(wasm)?;
    let build_metadata = parse_build_metadata(wasm)?;

    let comparison = match reference {
        None => ReproducibilityComparison {
            state: MetadataComparisonState::NotCompared,
            env_meta_equal: None,
            build_metadata: None,
            detail: "no reference artifact supplied; metadata comparison was not performed"
                .to_string(),
        },
        Some(reference) => {
            let reference_env = parse_env_meta(reference)?;
            let reference_build = parse_build_metadata(reference)?;
            let env_meta_equal = env_meta == reference_env;
            let (build_metadata, build_equal) = match (&build_metadata, &reference_build) {
                (Some(left), Some(right)) => {
                    let comparison = compare_build_metadata(left, right);
                    let equal = comparison.equal;
                    (Some(comparison), equal)
                }
                (None, None) => (None, true),
                _ => (None, false),
            };
            let equal = env_meta_equal && build_equal;
            ReproducibilityComparison {
                state: if equal {
                    MetadataComparisonState::Match
                } else {
                    MetadataComparisonState::Mismatch
                },
                env_meta_equal: Some(env_meta_equal),
                build_metadata,
                detail: if equal {
                    "embedded environment and build metadata match the reference artifact"
                        .to_string()
                } else {
                    "embedded environment or build metadata differs from the reference artifact"
                        .to_string()
                },
            }
        }
    };

    Ok(ReproducibilityReport {
        env_meta,
        build_metadata,
        comparison,
        source_reproduction: "not_performed".to_string(),
    })
}

/// Compare two build metadata documents.
#[must_use]
pub fn compare_build_metadata(left: &BuildMetadata, right: &BuildMetadata) -> MetadataComparison {
    let mut keys: Vec<&String> = left.entries.keys().chain(right.entries.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut differences = Vec::new();
    for key in &keys {
        let l = left.entries.get(*key);
        let r = right.entries.get(*key);
        if l != r {
            differences.push(MetadataDifference {
                key: (*key).clone(),
                left: l.cloned(),
                right: r.cloned(),
            });
        }
    }
    MetadataComparison {
        equal: differences.is_empty(),
        compared_keys: keys.len(),
        differences,
    }
}

/// Compare dependency metadata from two builds.
///
/// Differences are sorted by dependency name. When either build records no
/// dependency metadata, the result is [`DependencyComparisonState::Unavailable`]
/// rather than an empty difference list, because absence cannot be compared.
#[must_use]
pub fn compare_dependencies(left: &BuildMetadata, right: &BuildMetadata) -> DependencyComparison {
    let (Some(left), Some(right)) = (left.dependencies(), right.dependencies()) else {
        return unavailable_dependency_comparison(
            "dependency metadata is missing from one or both artifacts; dependency comparison was not performed",
        );
    };

    let mut names: Vec<&str> = left
        .iter()
        .chain(right.iter())
        .map(|dependency| dependency.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();

    let mut differences = Vec::new();
    for name in &names {
        let l = dependency_version(&left, name);
        let r = dependency_version(&right, name);
        if l != r {
            differences.push(DependencyDifference {
                name: (*name).to_string(),
                left: l,
                right: r,
            });
        }
    }

    let state = if differences.is_empty() {
        DependencyComparisonState::Match
    } else {
        DependencyComparisonState::Mismatch
    };
    DependencyComparison {
        state,
        compared_dependencies: names.len(),
        differences,
        detail: match state {
            DependencyComparisonState::Match => {
                "dependency metadata matches the reference artifact".to_string()
            }
            DependencyComparisonState::Mismatch => {
                "dependency metadata differs from the reference artifact".to_string()
            }
            DependencyComparisonState::Unavailable => unreachable!(),
        },
    }
}

fn dependency_version(dependencies: &[DependencyEntry], name: &str) -> Option<String> {
    dependencies
        .iter()
        .find(|dependency| dependency.name == name)
        .and_then(|dependency| dependency.version.clone())
}

fn unavailable_dependency_comparison(detail: &str) -> DependencyComparison {
    DependencyComparison {
        state: DependencyComparisonState::Unavailable,
        compared_dependencies: 0,
        differences: Vec::new(),
        detail: detail.to_string(),
    }
}

/// Build a dependency report from embedded metadata.
///
/// When `reference` is supplied, both artifacts' `dep:` entries are compared.
pub fn dependency_report(wasm: &[u8], reference: Option<&[u8]>) -> Result<DependencyReport> {
    let metadata = parse_build_metadata(wasm)?;
    let dependencies = metadata.as_ref().and_then(BuildMetadata::dependencies);
    let comparison = match (metadata.as_ref(), reference) {
        (_, None) => unavailable_dependency_comparison(
            "no reference artifact supplied; dependency comparison was not performed",
        ),
        (None, Some(_)) => unavailable_dependency_comparison(
            "this artifact has no build metadata; dependency comparison was not performed",
        ),
        (Some(left), Some(reference)) => match parse_build_metadata(reference)? {
            Some(right) => compare_dependencies(left, &right),
            None => unavailable_dependency_comparison(
                "the reference artifact has no build metadata; dependency comparison was not performed",
            ),
        },
    };
    Ok(DependencyReport {
        dependencies,
        comparison,
    })
}

/// Parse `contractenvmetav0` from a WASM artifact, if present.
pub fn parse_env_meta(wasm: &[u8]) -> Result<Option<EnvMeta>> {
    let Some(section) = observatory_wasm::extract_custom_section(
        wasm,
        observatory_wasm::CONTRACT_ENV_META_SECTION,
    )?
    else {
        return Ok(None);
    };
    let entries: Vec<ScEnvMetaEntry> = parse_stream(&section)?;
    if let Some(ScEnvMetaEntry::ScEnvMetaKindInterfaceVersion(version)) = entries.into_iter().next()
    {
        return Ok(Some(EnvMeta {
            protocol: version.protocol,
            pre_release: version.pre_release,
        }));
    }
    Ok(None)
}

/// Parse `contractmetav0` from a WASM artifact, if present.
pub fn parse_build_metadata(wasm: &[u8]) -> Result<Option<BuildMetadata>> {
    let Some(section) =
        observatory_wasm::extract_custom_section(wasm, observatory_wasm::CONTRACT_META_SECTION)?
    else {
        return Ok(None);
    };
    let entries: Vec<ScMetaEntry> = parse_stream(&section)?;
    let mut metadata = BuildMetadata::default();
    for entry in entries {
        let ScMetaEntry::ScMetaV0(ScMetaV0 { key, val }) = entry;
        metadata.entries.insert(key.to_string(), val.to_string());
    }
    Ok(Some(metadata))
}

/// Verify a local artifact against a deployed contract by executable hash.
pub fn verify_local_vs_deployed<T: Transport>(
    local_wasm: &[u8],
    client: &RpcClient<T>,
    contract_id: &str,
    network: &str,
) -> Result<VerificationResult> {
    let local_hash = observatory_wasm::artifact_fingerprint(local_wasm);
    let deployment = observatory_deployment::inspect(client, contract_id)?;

    let (status, detail) = match (deployment.found, deployment.wasm_hash.clone()) {
        (false, _) => (
            VerificationStatus::InsufficientData,
            "no contract instance entry was returned for the contract id".to_string(),
        ),
        (true, None) => (
            VerificationStatus::InsufficientData,
            "the deployed contract is not WASM-backed, so there is no hash to compare".to_string(),
        ),
        (true, Some(deployed)) if deployed == local_hash => (
            VerificationStatus::Match,
            "local artifact hash matches the deployed executable hash".to_string(),
        ),
        (true, Some(deployed)) => (
            VerificationStatus::Mismatch,
            format!("local artifact hash does not match the deployed hash {deployed}"),
        ),
    };

    Ok(VerificationResult {
        status,
        contract_id: contract_id.to_string(),
        network: network.to_string(),
        local_wasm_hash: local_hash,
        deployed_wasm_hash: deployment.wasm_hash,
        detail,
    })
}

/// Aggregate counts for a batch verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct BatchSummary {
    /// Number of artifacts verified.
    pub total: usize,
    /// Artifacts whose hashes matched.
    pub matched: usize,
    /// Artifacts whose hashes did not match.
    pub mismatched: usize,
    /// Artifacts with insufficient data.
    pub insufficient_data: usize,
    /// Artifacts whose verification could not be completed.
    pub errors: usize,
}

/// The summarized result of verifying several artifacts in one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchVerification {
    /// Per-artifact results, in manifest order.
    pub results: Vec<VerificationResult>,
    /// Aggregate counts.
    pub summary: BatchSummary,
}

impl BatchVerification {
    /// Summarize per-artifact results.
    #[must_use]
    pub fn from_results(results: Vec<VerificationResult>) -> Self {
        let mut summary = BatchSummary {
            total: results.len(),
            ..BatchSummary::default()
        };
        for result in &results {
            match result.status {
                VerificationStatus::Match => summary.matched += 1,
                VerificationStatus::Mismatch => summary.mismatched += 1,
                VerificationStatus::InsufficientData => summary.insufficient_data += 1,
                VerificationStatus::Error => summary.errors += 1,
            }
        }
        BatchVerification { results, summary }
    }

    /// The aggregate status: errors dominate, then mismatches, then missing data.
    #[must_use]
    pub fn status(&self) -> VerificationStatus {
        if self.summary.errors > 0 {
            VerificationStatus::Error
        } else if self.summary.mismatched > 0 {
            VerificationStatus::Mismatch
        } else if self.summary.insufficient_data > 0 {
            VerificationStatus::InsufficientData
        } else {
            VerificationStatus::Match
        }
    }
}

/// A source of deployed contract code, keyed by executable hash.
///
/// Live RPC fetching is blocked on issue #14; this trait keeps the interface
/// comparison independent of the transport so callers and tests can inject a
/// deterministic source. Implementations must never execute the returned code.
pub trait DeployedCodeSource {
    /// Return the deployed WASM for `wasm_hash`, or `None` when it is missing.
    fn deployed_wasm(&self, wasm_hash: &str) -> Result<Option<Vec<u8>>>;
}

impl<T: Transport> DeployedCodeSource for RpcClient<T> {
    fn deployed_wasm(&self, wasm_hash: &str) -> Result<Option<Vec<u8>>> {
        observatory_deployment::deployed_wasm(self, wasm_hash)
    }
}

/// Verification that separates artifact identity from interface identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceVerification {
    /// The hash-based artifact verification.
    pub artifact: VerificationResult,
    /// Whether the local and deployed interfaces are identical, when both are
    /// known.
    pub interface_match: Option<bool>,
    /// Canonical fingerprint of the local interface, when present.
    pub local_interface_sha256: Option<String>,
    /// Canonical fingerprint of the deployed interface, when present.
    pub deployed_interface_sha256: Option<String>,
    /// Interface diff with the local interface as the baseline, when both are
    /// known.
    pub diff: Option<InterfaceDiff>,
    /// Compatibility of the deployed interface against the local interface,
    /// when both are known.
    pub compatibility: Option<CompatibilityAssessment>,
    /// Always `not_performed`: this comparison never verifies source.
    pub source_verification: String,
    /// A human-readable explanation.
    pub detail: String,
}

/// Compare a local artifact's interface against the deployed contract's code.
///
/// This never claims source-level verification: it compares interface identity
/// (canonical fingerprints), separately from the artifact hash comparison.
pub fn verify_interface<T: Transport>(
    local_wasm: &[u8],
    client: &RpcClient<T>,
    contract_id: &str,
    network: &str,
) -> Result<InterfaceVerification> {
    let artifact = verify_local_vs_deployed(local_wasm, client, contract_id, network)?;
    verify_interface_with_source(local_wasm, artifact, client)
}

/// Compare interfaces using an injected deployed-code source.
///
/// The artifact identity comes from `artifact`; the deployed interface is loaded
/// through `source` so the comparison can be tested without RPC.
pub fn verify_interface_with_source<S: DeployedCodeSource>(
    local_wasm: &[u8],
    artifact: VerificationResult,
    source: &S,
) -> Result<InterfaceVerification> {
    let local_interface = ContractInterface::from_wasm(local_wasm).ok();
    let deployed_interface = match &artifact.deployed_wasm_hash {
        Some(hash) => source
            .deployed_wasm(hash)?
            .and_then(|bytes| ContractInterface::from_wasm(&bytes).ok()),
        None => None,
    };

    let local_fingerprint = local_interface
        .as_ref()
        .map(ContractInterface::fingerprint)
        .transpose()?;
    let deployed_fingerprint = deployed_interface
        .as_ref()
        .map(ContractInterface::fingerprint)
        .transpose()?;

    let (interface_match, detail) = match (&local_fingerprint, &deployed_fingerprint) {
        (Some(local), Some(deployed)) if local == deployed => (
            Some(true),
            "local and deployed interfaces are identical".to_string(),
        ),
        (Some(_), Some(_)) => (
            Some(false),
            "local and deployed interfaces differ".to_string(),
        ),
        (None, _) => (
            None,
            "local artifact has no contract specification to compare".to_string(),
        ),
        (Some(_), None) => (
            None,
            "deployed code could not be fetched or has no contract specification".to_string(),
        ),
    };

    let (interface_diff, compatibility) = match (&local_interface, &deployed_interface) {
        (Some(local), Some(deployed)) => (
            Some(diff(local, deployed)),
            Some(assess(local, deployed, CompatibilityPolicy::strict())),
        ),
        _ => (None, None),
    };

    Ok(InterfaceVerification {
        artifact,
        interface_match,
        local_interface_sha256: local_fingerprint,
        deployed_interface_sha256: deployed_fingerprint,
        diff: interface_diff,
        compatibility,
        source_verification: "not_performed".to_string(),
        detail,
    })
}

fn parse_stream<T: ReadXdr>(bytes: &[u8]) -> Result<Vec<T>> {
    let mut limited = Limited::new(Cursor::new(bytes), Limits::none());
    let mut entries = Vec::new();
    while (limited.inner.position() as usize) < bytes.len() {
        if entries.len() >= MAX_SPEC_ENTRIES {
            return Err(ObservatoryError::decode(
                "metadata stream exceeds the entry limit",
            ));
        }
        let entry = T::read_xdr(&mut limited).map_err(|error| {
            ObservatoryError::decode(format!("invalid metadata entry: {error}"))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_rpc::{Endpoint, MockTransport};
    use observatory_testutil::spec::spec_function;
    use observatory_testutil::{custom_section, minimal_module, module_with_spec};
    use serde_json::json;
    use stellar_xdr::{
        ContractCodeCostInputs, ContractCodeEntry, ContractCodeEntryExt, ContractCodeEntryV1,
        ContractDataDurability, ContractDataEntry, ContractExecutable, ContractId, ExtensionPoint,
        Hash, LedgerEntryData, ScAddress, ScContractInstance, ScSpecTypeDef, ScVal, WriteXdr,
    };

    fn module_with_env_meta() -> Vec<u8> {
        let entry = ScEnvMetaEntry::ScEnvMetaKindInterfaceVersion(
            stellar_xdr::ScEnvMetaEntryInterfaceVersion {
                protocol: 22,
                pre_release: 0,
            },
        );
        let payload = entry.to_xdr(Limits::none()).unwrap();
        let mut module = minimal_module();
        module.extend(custom_section(
            observatory_wasm::CONTRACT_ENV_META_SECTION,
            &payload,
        ));
        module
    }

    fn module_with_build_meta() -> Vec<u8> {
        let entries = vec![
            ScMetaEntry::ScMetaV0(ScMetaV0 {
                key: "rsver".try_into().unwrap(),
                val: "1.85.0".try_into().unwrap(),
            }),
            ScMetaEntry::ScMetaV0(ScMetaV0 {
                key: "rssdkver".try_into().unwrap(),
                val: "22.0.0".try_into().unwrap(),
            }),
        ];
        let mut payload = Vec::new();
        for entry in entries {
            payload.extend(entry.to_xdr(Limits::none()).unwrap());
        }
        let mut module = minimal_module();
        module.extend(custom_section(
            observatory_wasm::CONTRACT_META_SECTION,
            &payload,
        ));
        module
    }

    #[test]
    fn parses_env_meta() {
        let meta = parse_env_meta(&module_with_env_meta()).unwrap().unwrap();
        assert_eq!(meta.protocol, 22);
        assert_eq!(parse_env_meta(&minimal_module()).unwrap(), None);
    }

    #[test]
    fn parses_build_metadata() {
        let metadata = parse_build_metadata(&module_with_build_meta())
            .unwrap()
            .unwrap();
        assert_eq!(metadata.get("rsver"), Some("1.85.0"));
        assert_eq!(metadata.get("rssdkver"), Some("22.0.0"));
    }

    #[test]
    fn exposes_known_build_metadata_keys_and_preserves_unknown_ones() {
        let metadata = BuildMetadata::from_json(&json!({
            "rsver": "1.85.0",
            "rssdkver": "22.0.0",
            "target": "wasm32v1-none",
            "profile": "release",
            "custom_key": "kept"
        }))
        .unwrap();

        assert_eq!(metadata.compiler(), Some("1.85.0"));
        assert_eq!(metadata.sdk(), Some("22.0.0"));
        assert_eq!(metadata.target(), Some("wasm32v1-none"));
        assert_eq!(metadata.profile(), Some("release"));
        assert_eq!(metadata.get("custom_key"), Some("kept"));

        let round_tripped: BuildMetadata =
            serde_json::from_value(serde_json::to_value(&metadata).unwrap()).unwrap();
        assert_eq!(round_tripped, metadata);
        assert_eq!(round_tripped.get("custom_key"), Some("kept"));
    }

    #[test]
    fn missing_build_metadata_keys_are_none() {
        let metadata = BuildMetadata::from_json(&json!({ "custom_key": "only" })).unwrap();
        assert_eq!(metadata.compiler(), None);
        assert_eq!(metadata.sdk(), None);
        assert_eq!(metadata.target(), None);
        assert_eq!(metadata.profile(), None);
    }

    #[test]
    fn compares_build_metadata() {
        let a =
            BuildMetadata::from_json(&json!({ "rsver": "1.85.0", "profile": "release" })).unwrap();
        let b = BuildMetadata::from_json(&json!({ "rsver": "1.86.0" })).unwrap();
        let comparison = compare_build_metadata(&a, &b);
        assert!(!comparison.equal);
        assert_eq!(comparison.compared_keys, 2);
        assert_eq!(comparison.differences.len(), 2);

        let same = compare_build_metadata(&a, &a);
        assert!(same.equal);
        assert!(same.differences.is_empty());
    }

    #[test]
    fn parses_dependency_entries_and_preserves_other_keys() {
        let metadata = BuildMetadata::from_json(&json!({
            "dep:serde": "1.0.200",
            "dep:soroban-sdk": "22.0.0#abc123",
            "dep:empty": "",
            "rsver": "1.85.0",
            "custom": "kept"
        }))
        .unwrap();

        let dependencies = metadata.dependencies().unwrap();
        assert_eq!(
            dependencies
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            vec!["empty", "serde", "soroban-sdk"]
        );
        assert_eq!(dependencies[0].version, None);
        assert_eq!(dependencies[1].version.as_deref(), Some("1.0.200"));
        assert_eq!(dependencies[2].version.as_deref(), Some("22.0.0#abc123"));
        assert_eq!(metadata.get("rsver"), Some("1.85.0"));
        assert_eq!(metadata.get("custom"), Some("kept"));

        let without = BuildMetadata::from_json(&json!({ "rsver": "1.85.0" })).unwrap();
        assert!(without.dependencies().is_none());
    }

    #[test]
    fn compares_dependencies_deterministically() {
        let left = BuildMetadata::from_json(&json!({
            "dep:zeta": "1",
            "dep:alpha": "1",
            "dep:beta": "1"
        }))
        .unwrap();
        let right = BuildMetadata::from_json(&json!({
            "dep:zeta": "2",
            "dep:alpha": "1",
            "dep:beta": "2"
        }))
        .unwrap();

        let comparison = compare_dependencies(&left, &right);
        assert_eq!(comparison.state, DependencyComparisonState::Mismatch);
        assert_eq!(comparison.compared_dependencies, 3);
        assert_eq!(
            comparison
                .differences
                .iter()
                .map(|difference| difference.name.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "zeta"]
        );
        assert_eq!(comparison, compare_dependencies(&left, &right));

        let same = compare_dependencies(&left, &left);
        assert_eq!(same.state, DependencyComparisonState::Match);
        assert!(same.differences.is_empty());
    }

    #[test]
    fn dependency_comparison_is_unavailable_without_data() {
        let with = BuildMetadata::from_json(&json!({ "dep:serde": "1.0.200" })).unwrap();
        let without = BuildMetadata::from_json(&json!({ "rsver": "1.85.0" })).unwrap();

        let comparison = compare_dependencies(&with, &without);
        assert_eq!(comparison.state, DependencyComparisonState::Unavailable);
        assert_eq!(comparison.compared_dependencies, 0);
        assert!(comparison.differences.is_empty());

        let report = dependency_report(&minimal_module(), None).unwrap();
        assert!(report.dependencies.is_none());
        assert_eq!(
            report.comparison.state,
            DependencyComparisonState::Unavailable
        );
    }

    fn module_with_all_metadata() -> Vec<u8> {
        let mut module = module_with_build_meta();
        module.extend(
            module_with_env_meta()
                .into_iter()
                .skip(minimal_module().len()),
        );
        module
    }

    #[test]
    fn reproducibility_report_reports_without_claiming_source_reproduction() {
        let wasm = module_with_all_metadata();
        let report = reproducibility_report(&wasm, None).unwrap();
        assert_eq!(report.env_meta.map(|meta| meta.protocol), Some(22));
        assert_eq!(
            report
                .build_metadata
                .as_ref()
                .and_then(|metadata| metadata.get("rsver")),
            Some("1.85.0")
        );
        assert_eq!(
            report.comparison.state,
            MetadataComparisonState::NotCompared
        );
        assert_eq!(report.source_reproduction, "not_performed");
    }

    #[test]
    fn reproducibility_report_compares_a_reference_artifact() {
        let wasm = module_with_all_metadata();
        let matching = reproducibility_report(&wasm, Some(&wasm)).unwrap();
        assert_eq!(matching.comparison.state, MetadataComparisonState::Match);
        assert_eq!(matching.comparison.env_meta_equal, Some(true));
        assert_eq!(
            matching
                .comparison
                .build_metadata
                .as_ref()
                .map(|comparison| comparison.equal),
            Some(true)
        );

        let differing = reproducibility_report(&wasm, Some(&minimal_module())).unwrap();
        assert_eq!(
            differing.comparison.state,
            MetadataComparisonState::Mismatch
        );
        assert_eq!(differing.comparison.env_meta_equal, Some(false));
    }

    fn deployed_transport(hash_byte: u8) -> MockTransport {
        let data = LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash([9u8; 32]))),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
            val: ScVal::ContractInstance(ScContractInstance {
                executable: ContractExecutable::Wasm(Hash([hash_byte; 32])),
                storage: None,
            }),
        });
        MockTransport::new().with_result(
            "getLedgerEntries",
            json!({ "entries": [ { "key": "k", "xdr": data.to_xdr_base64(Limits::none()).unwrap() } ] }),
        )
    }

    #[test]
    fn verifies_match_and_mismatch() {
        let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
        let local = minimal_module();
        let local_hash = observatory_wasm::artifact_fingerprint(&local);

        // Deployed hash equal to local: build a transport whose decoded hash we control.
        let matching = MockTransport::new().with_result(
            "getLedgerEntries",
            json!({ "entries": [ { "key": "k", "xdr": wasm_instance_xdr(&local_hash) } ] }),
        );
        let client = RpcClient::new(Endpoint::parse("https://example.org").unwrap(), matching);
        let result = verify_local_vs_deployed(&local, &client, &contract_id, "testnet").unwrap();
        assert_eq!(result.status, VerificationStatus::Match);

        let mismatching = deployed_transport(1);
        let client = RpcClient::new(Endpoint::parse("https://example.org").unwrap(), mismatching);
        let result = verify_local_vs_deployed(&local, &client, &contract_id, "testnet").unwrap();
        assert_eq!(result.status, VerificationStatus::Mismatch);
    }

    /// Build a ledger entry whose executable is the WASM hash derived from
    /// `hex_hash`; used to exercise the match path deterministically.
    fn wasm_instance_xdr(hex_hash: &str) -> String {
        let bytes = hex::decode(hex_hash).unwrap();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
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

    #[test]
    fn missing_deployment_is_insufficient_data() {
        let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
        let transport =
            MockTransport::new().with_result("getLedgerEntries", json!({ "entries": [] }));
        let client = RpcClient::new(Endpoint::parse("https://example.org").unwrap(), transport);
        let result =
            verify_local_vs_deployed(&minimal_module(), &client, &contract_id, "testnet").unwrap();
        assert_eq!(result.status, VerificationStatus::InsufficientData);
    }

    fn instance_xdr(hash: [u8; 32]) -> String {
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

    fn code_xdr(wasm: &[u8], hash: [u8; 32]) -> String {
        let data = LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V1(ContractCodeEntryV1 {
                ext: ExtensionPoint::V0,
                cost_inputs: ContractCodeCostInputs {
                    ext: ExtensionPoint::V0,
                    n_instructions: 1,
                    n_functions: 1,
                    n_globals: 0,
                    n_table_entries: 0,
                    n_types: 0,
                    n_data_segments: 0,
                    n_elem_segments: 0,
                    n_imports: 0,
                    n_exports: 1,
                    n_data_segment_bytes: 0,
                },
            }),
            hash: Hash(hash),
            code: wasm.to_vec().try_into().unwrap(),
        });
        data.to_xdr_base64(Limits::none()).unwrap()
    }

    #[test]
    fn compares_local_and_deployed_interfaces() {
        let wasm = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
        let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
        let transport = MockTransport::new().with_sequence(
            "getLedgerEntries",
            vec![
                json!({ "entries": [ { "key": "", "xdr": instance_xdr([7u8; 32]) } ] }),
                json!({ "entries": [ { "key": "", "xdr": code_xdr(&wasm, [7u8; 32]) } ] }),
            ],
        );
        let client = RpcClient::new(Endpoint::parse("https://example.org").unwrap(), transport);
        let result = verify_interface(&wasm, &client, &contract_id, "testnet").unwrap();
        assert_eq!(result.interface_match, Some(true));
        assert!(result.local_interface_sha256.is_some());
        assert_eq!(
            result.local_interface_sha256,
            result.deployed_interface_sha256
        );
    }

    #[test]
    fn detects_differing_deployed_interface() {
        let local = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
        let deployed =
            module_with_spec(&[spec_function("goodbye", &[], Some(ScSpecTypeDef::Bool))]);
        let contract_id = format!("{}", stellar_strkey::Contract([9u8; 32]));
        let transport = MockTransport::new().with_sequence(
            "getLedgerEntries",
            vec![
                json!({ "entries": [ { "key": "", "xdr": instance_xdr([7u8; 32]) } ] }),
                json!({ "entries": [ { "key": "", "xdr": code_xdr(&deployed, [7u8; 32]) } ] }),
            ],
        );
        let client = RpcClient::new(Endpoint::parse("https://example.org").unwrap(), transport);
        let result = verify_interface(&local, &client, &contract_id, "testnet").unwrap();
        assert_eq!(result.interface_match, Some(false));
    }

    struct MockSource(Option<Vec<u8>>);

    impl DeployedCodeSource for MockSource {
        fn deployed_wasm(&self, _wasm_hash: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.0.clone())
        }
    }

    fn artifact_for(local_wasm: &[u8]) -> VerificationResult {
        VerificationResult {
            status: VerificationStatus::Mismatch,
            contract_id: "C...".to_string(),
            network: "testnet".to_string(),
            local_wasm_hash: observatory_wasm::artifact_fingerprint(local_wasm),
            deployed_wasm_hash: Some("00".repeat(32)),
            detail: String::new(),
        }
    }

    #[test]
    fn injected_source_reports_matching_interfaces() {
        let wasm = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
        let result = verify_interface_with_source(
            &wasm,
            artifact_for(&wasm),
            &MockSource(Some(wasm.clone())),
        )
        .unwrap();
        assert_eq!(result.interface_match, Some(true));
        assert!(result.diff.as_ref().unwrap().changes.is_empty());
        assert_eq!(
            result.compatibility.as_ref().unwrap().status,
            observatory_compat::CompatibilityStatus::Compatible
        );
        assert_eq!(result.source_verification, "not_performed");
    }

    #[test]
    fn injected_source_reports_interface_differences_and_compatibility() {
        let local = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
        let deployed = module_with_spec(&[spec_function(
            "goodbye",
            &[("value", ScSpecTypeDef::U32)],
            Some(ScSpecTypeDef::Bool),
        )]);
        let result =
            verify_interface_with_source(&local, artifact_for(&local), &MockSource(Some(deployed)))
                .unwrap();
        assert_eq!(result.interface_match, Some(false));
        assert!(!result.diff.as_ref().unwrap().changes.is_empty());
        assert_eq!(
            result.compatibility.as_ref().unwrap().status,
            observatory_compat::CompatibilityStatus::Incompatible
        );
        assert_eq!(result.source_verification, "not_performed");
    }

    #[test]
    fn injected_source_reports_missing_deployed_code() {
        let local = module_with_spec(&[spec_function("hello", &[], Some(ScSpecTypeDef::Bool))]);
        let result =
            verify_interface_with_source(&local, artifact_for(&local), &MockSource(None)).unwrap();
        assert_eq!(result.interface_match, None);
        assert!(result.diff.is_none());
        assert!(result.compatibility.is_none());
        assert!(result.detail.contains("could not be fetched"));
    }
}
