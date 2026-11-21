//! Canonical contract interface model and deterministic fingerprinting.
//!
//! The interface is derived from the parsed [`observatory_spec::ContractSpec`]
//! and canonicalized so that two semantically identical interfaces produce the
//! same fingerprint regardless of declaration order.
//!
//! # Ordering semantics
//!
//! * Functions, structs, unions, enums, error enums, and events are **sorted by
//!   name**; their declaration order is not semantic.
//! * Function parameters, struct fields, union cases, and enum cases **retain
//!   their declared order**, which is semantic for XDR encoding and call ABI.
//!
//! A fingerprint covers the *interface only*. Two contracts with the same
//! interface can have different implementations, so interface equality does not
//! imply implementation equality.

use observatory_core::hash::sha256_hex;
use observatory_core::{ObservatoryError, Result};
use observatory_spec::{
    ContractSpec, SpecEnum, SpecErrorEnum, SpecEvent, SpecFunction, SpecStruct, SpecType, SpecUnion,
};
use serde::{Deserialize, Serialize};

/// Version of the canonical interface representation.
pub const INTERFACE_SCHEMA_VERSION: &str = "1.0";

/// A canonical, order-independent view of a contract's public interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractInterface {
    /// Schema version of this representation.
    pub schema_version: String,
    /// Functions, sorted by name.
    pub functions: Vec<SpecFunction>,
    /// Structs, sorted by name.
    pub structs: Vec<SpecStruct>,
    /// Unions, sorted by name.
    pub unions: Vec<SpecUnion>,
    /// Enums, sorted by name.
    pub enums: Vec<SpecEnum>,
    /// Error enums, sorted by name.
    pub error_enums: Vec<SpecErrorEnum>,
    /// Events, sorted by name.
    pub events: Vec<SpecEvent>,
}

impl ContractInterface {
    /// Build a canonical interface from a normalized specification.
    #[must_use]
    pub fn from_spec(spec: ContractSpec) -> Self {
        let mut interface = ContractInterface {
            schema_version: INTERFACE_SCHEMA_VERSION.to_string(),
            functions: spec.functions,
            structs: spec.structs,
            unions: spec.unions,
            enums: spec.enums,
            error_enums: spec.error_enums,
            events: spec.events,
        };
        interface.canonicalize();
        interface
    }

    /// Parse a contract specification directly from WASM bytes.
    pub fn from_wasm(wasm: &[u8]) -> Result<Self> {
        Ok(ContractInterface::from_spec(observatory_spec::parse_wasm(
            wasm,
        )?))
    }

    /// Parse a contract interface from a raw specification section.
    pub fn from_spec_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(ContractInterface::from_spec(observatory_spec::parse(
            bytes,
        )?))
    }

    /// Sort all name-keyed collections. Field/parameter/case order is preserved.
    pub fn canonicalize(&mut self) {
        self.functions.sort_by(|a, b| a.name.cmp(&b.name));
        self.structs.sort_by(|a, b| a.name.cmp(&b.name));
        self.unions.sort_by(|a, b| a.name.cmp(&b.name));
        self.enums.sort_by(|a, b| a.name.cmp(&b.name));
        self.error_enums.sort_by(|a, b| a.name.cmp(&b.name));
        self.events.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Whether the interface declares nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.structs.is_empty()
            && self.unions.is_empty()
            && self.enums.is_empty()
            && self.error_enums.is_empty()
            && self.events.is_empty()
    }

    /// Find a function by name.
    #[must_use]
    pub fn function(&self, name: &str) -> Option<&SpecFunction> {
        self.functions.iter().find(|function| function.name == name)
    }

    /// Find a user-defined type by name across all type kinds.
    #[must_use]
    pub fn has_type(&self, name: &str) -> bool {
        self.structs.iter().any(|item| item.name == name)
            || self.unions.iter().any(|item| item.name == name)
            || self.enums.iter().any(|item| item.name == name)
            || self.error_enums.iter().any(|item| item.name == name)
    }

    /// All type names, sorted.
    #[must_use]
    pub fn type_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .structs
            .iter()
            .map(|item| item.name.clone())
            .chain(self.unions.iter().map(|item| item.name.clone()))
            .chain(self.enums.iter().map(|item| item.name.clone()))
            .chain(self.error_enums.iter().map(|item| item.name.clone()))
            .collect();
        names.sort();
        names
    }

    /// Total number of interface entities.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.functions.len()
            + self.structs.len()
            + self.unions.len()
            + self.enums.len()
            + self.error_enums.len()
            + self.events.len()
    }

    /// The canonical JSON encoding used for fingerprinting.
    ///
    /// `serde_json` orders object keys deterministically, so this encoding is
    /// stable for a given interface.
    pub fn canonical_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|error| {
            ObservatoryError::internal(format!("interface serialization: {error}"))
        })
    }

    /// The deterministic SHA-256 fingerprint of the canonical interface.
    pub fn fingerprint(&self) -> Result<String> {
        Ok(sha256_hex(&self.canonical_json()?))
    }

    /// Reconstruct a normalized specification for validation or summarization.
    #[must_use]
    pub fn to_spec(&self) -> ContractSpec {
        ContractSpec {
            functions: self.functions.clone(),
            structs: self.structs.clone(),
            unions: self.unions.clone(),
            enums: self.enums.clone(),
            error_enums: self.error_enums.clone(),
            events: self.events.clone(),
        }
    }
}

/// Whether `ty` references a named user-defined type.
#[must_use]
pub fn type_references_udt(ty: &SpecType) -> bool {
    match ty {
        SpecType::Udt { .. } => true,
        SpecType::Option(inner) | SpecType::Vec(inner) => type_references_udt(inner),
        SpecType::Result { ok, error } => type_references_udt(ok) || type_references_udt(error),
        SpecType::Map { key, value } => type_references_udt(key) || type_references_udt(value),
        SpecType::Tuple(types) => types.iter().any(type_references_udt),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_spec::normalize;
    use observatory_testutil::spec::{spec_enum, spec_function, spec_struct, ty_udt};
    use stellar_xdr::{ScSpecEntry, ScSpecTypeDef};

    fn spec(entries: Vec<ScSpecEntry>) -> ContractSpec {
        normalize(entries)
    }

    #[test]
    fn canonicalizes_by_name() {
        let a = ContractInterface::from_spec(spec(vec![
            spec_function("b", &[], None),
            spec_function("a", &[], None),
        ]));
        let b = ContractInterface::from_spec(spec(vec![
            spec_function("a", &[], None),
            spec_function("b", &[], None),
        ]));
        let names: Vec<_> = a.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    }

    #[test]
    fn fingerprint_changes_with_interface() {
        let a = ContractInterface::from_spec(spec(vec![spec_function(
            "f",
            &[("x", ScSpecTypeDef::U32)],
            None,
        )]));
        let b = ContractInterface::from_spec(spec(vec![spec_function(
            "f",
            &[("x", ScSpecTypeDef::I64)],
            None,
        )]));
        assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    }

    #[test]
    fn parameter_order_is_semantic() {
        let a = ContractInterface::from_spec(spec(vec![spec_function(
            "f",
            &[("x", ScSpecTypeDef::U32), ("y", ScSpecTypeDef::U32)],
            None,
        )]));
        let b = ContractInterface::from_spec(spec(vec![spec_function(
            "f",
            &[("y", ScSpecTypeDef::U32), ("x", ScSpecTypeDef::U32)],
            None,
        )]));
        assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    }

    #[test]
    fn type_lookup_and_counts() {
        let interface = ContractInterface::from_spec(spec(vec![
            spec_function("f", &[("s", ty_udt("S"))], None),
            spec_struct("S", &[("a", ScSpecTypeDef::U32)]),
            spec_enum("E", &[("A", 0)]),
        ]));
        assert!(interface.has_type("S"));
        assert!(interface.has_type("E"));
        assert!(!interface.has_type("Missing"));
        assert_eq!(interface.entity_count(), 3);
        assert_eq!(
            interface.type_names(),
            vec!["E".to_string(), "S".to_string()]
        );
    }

    #[test]
    fn empty_interface_is_empty() {
        let interface = ContractInterface::from_spec(ContractSpec::default());
        assert!(interface.is_empty());
        assert_eq!(interface.entity_count(), 0);
    }

    #[test]
    fn udt_reference_detection() {
        assert!(type_references_udt(&SpecType::Udt { name: "S".into() }));
        assert!(type_references_udt(&SpecType::Vec(Box::new(
            SpecType::Udt { name: "S".into() }
        ))));
        assert!(!type_references_udt(&SpecType::U32));
    }

    #[test]
    fn known_fingerprint_vector() {
        let interface = ContractInterface::from_spec(spec(vec![spec_function(
            "ping",
            &[("n", ScSpecTypeDef::U32)],
            Some(ScSpecTypeDef::Bool),
        )]));
        assert_eq!(
            interface.fingerprint().unwrap(),
            "55b4750964574865fa8ecb0a1b6c772fd9b666cf5e6f556832011ad1f3a0f1d4"
        );
    }

    /// Deterministic interfaces covering the shapes that the canonical
    /// serialization must handle: primitives, composites, user-defined types,
    /// events, and a large interface.
    fn known_vectors() -> Vec<(&'static str, ContractInterface)> {
        use observatory_testutil::spec::{spec_error_enum, spec_event, spec_union};
        use stellar_xdr::{
            ScSpecEventParamLocationV0, ScSpecTypeBytesN, ScSpecTypeMap, ScSpecTypeOption,
            ScSpecTypeResult, ScSpecTypeTuple, ScSpecTypeVec, VecM,
        };

        let primitives = ContractInterface::from_spec(spec(vec![spec_function(
            "primitives",
            &[
                ("bool", ScSpecTypeDef::Bool),
                ("u32", ScSpecTypeDef::U32),
                ("i32", ScSpecTypeDef::I32),
                ("u64", ScSpecTypeDef::U64),
                ("i64", ScSpecTypeDef::I64),
                ("u128", ScSpecTypeDef::U128),
                ("i128", ScSpecTypeDef::I128),
                ("symbol", ScSpecTypeDef::Symbol),
                ("string", ScSpecTypeDef::String),
                ("bytes", ScSpecTypeDef::Bytes),
                ("address", ScSpecTypeDef::Address),
            ],
            Some(ScSpecTypeDef::Void),
        )]));

        let composites = ContractInterface::from_spec(spec(vec![spec_function(
            "composites",
            &[
                (
                    "opt",
                    ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
                        value_type: Box::new(ScSpecTypeDef::U32),
                    })),
                ),
                (
                    "vec",
                    ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
                        element_type: Box::new(ScSpecTypeDef::I128),
                    })),
                ),
                (
                    "map",
                    ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
                        key_type: Box::new(ScSpecTypeDef::Symbol),
                        value_type: Box::new(ScSpecTypeDef::U64),
                    })),
                ),
                (
                    "res",
                    ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
                        ok_type: Box::new(ScSpecTypeDef::U32),
                        error_type: Box::new(ty_udt("Error")),
                    })),
                ),
                (
                    "tup",
                    ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
                        value_types: VecM::try_from(vec![ScSpecTypeDef::U32, ScSpecTypeDef::Bool])
                            .expect("within limit"),
                    })),
                ),
                ("bytesn", ScSpecTypeDef::BytesN(ScSpecTypeBytesN { n: 32 })),
            ],
            None,
        )]));

        let udts = ContractInterface::from_spec(spec(vec![
            spec_struct(
                "Config",
                &[
                    ("owner", ScSpecTypeDef::Address),
                    ("limit", ScSpecTypeDef::U32),
                ],
            ),
            spec_enum("Status", &[("Active", 0), ("Paused", 1)]),
            spec_union(
                "Payload",
                &[("None", None), ("Some", Some(vec![ty_udt("Config")]))],
            ),
            spec_error_enum("Error", &[("NotFound", 1), ("Denied", 2)]),
            spec_function(
                "store",
                &[("config", ty_udt("Config"))],
                Some(ty_udt("Status")),
            ),
        ]));

        let events = ContractInterface::from_spec(spec(vec![
            spec_event(
                "transfer",
                &["transfer", "v1"],
                &[
                    (
                        "from",
                        ScSpecTypeDef::Address,
                        ScSpecEventParamLocationV0::TopicList,
                    ),
                    (
                        "amount",
                        ScSpecTypeDef::I128,
                        ScSpecEventParamLocationV0::Data,
                    ),
                ],
            ),
            spec_event(
                "mint",
                &["mint"],
                &[(
                    "to",
                    ScSpecTypeDef::Address,
                    ScSpecEventParamLocationV0::Data,
                )],
            ),
        ]));

        let mut large_entries = Vec::new();
        for index in 0..20 {
            large_entries.push(spec_function(
                &format!("fn_{index:02}"),
                &[("value", ScSpecTypeDef::U64)],
                Some(ScSpecTypeDef::Bool),
            ));
            large_entries.push(spec_struct(
                &format!("Type{index:02}"),
                &[("field", ScSpecTypeDef::U32)],
            ));
        }
        let large = ContractInterface::from_spec(spec(large_entries));

        vec![
            ("primitives", primitives),
            ("composites", composites),
            ("udts", udts),
            ("events", events),
            ("large", large),
        ]
    }

    #[test]
    fn known_fingerprint_vectors_are_stable() {
        // Each vector pins the canonical JSON serialization *format*, not
        // interface semantics. A serialization change that alters the bytes for
        // any of these interfaces must update these digests deliberately.
        let expected = [
            (
                "primitives",
                "a36fd01c938664e61ca7ba28b3f699cdab1e8eba8eae92758a4cad8ba278dd70",
            ),
            (
                "composites",
                "c22fb5358b7caf19b2ebc0255ea51db50edd29e9ea24408d3f4f050f70ef37ab",
            ),
            (
                "udts",
                "fc7c9ae5bc2773533bf3ae7ffd6ba8afd124284e92daa574205edcd81f108bf3",
            ),
            (
                "events",
                "bfa29909a24d3e6b9c35dda8c65db6c8f1bd8a0aeac47e53a8e82c6f8660e5fb",
            ),
            (
                "large",
                "0e2c381f2903816551f613f07556660759a97e1836c5574dac2d2dd747ff3d09",
            ),
        ];

        let vectors = known_vectors();
        assert_eq!(vectors.len(), expected.len());
        for ((name, interface), (expected_name, digest)) in vectors.iter().zip(expected.iter()) {
            assert_eq!(name, expected_name);
            assert_eq!(
                interface.fingerprint().unwrap(),
                *digest,
                "fingerprint vector `{name}` changed; update the digest only for a deliberate serialization change"
            );
        }
    }
}
