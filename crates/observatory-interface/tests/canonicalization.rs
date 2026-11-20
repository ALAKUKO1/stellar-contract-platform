//! Property tests for canonicalization.
//!
//! Interfaces are generated deterministically from the shared XDR fixture
//! builders. The case count is fixed, so these tests never depend on wall
//! clock, randomness sources, or the network. Failures shrink to a minimal set
//! of spec entries before they are reported.

use observatory_interface::ContractInterface;
use observatory_spec::normalize;
use observatory_testutil::spec::{
    spec_enum, spec_error_enum, spec_event, spec_function, spec_struct, spec_union, ty_udt,
};
use stellar_xdr::{ScSpecEntry, ScSpecEventParamLocationV0, ScSpecTypeDef};

/// A tiny deterministic xorshift generator; enough for fixture variation.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, upper: usize) -> usize {
        (self.next() % upper as u64) as usize
    }
}

fn ty(rng: &mut Rng) -> ScSpecTypeDef {
    match rng.below(6) {
        0 => ScSpecTypeDef::U32,
        1 => ScSpecTypeDef::I64,
        2 => ScSpecTypeDef::Bool,
        3 => ScSpecTypeDef::Address,
        4 => ty_udt("Payload"),
        _ => ScSpecTypeDef::I128,
    }
}

fn generate(rng: &mut Rng) -> Vec<ScSpecEntry> {
    let mut entries = Vec::new();

    for index in 0..1 + rng.below(4) {
        let params = (0..rng.below(3))
            .map(|param| (format!("p{param}"), ty(rng)))
            .collect::<Vec<_>>();
        let params = params
            .iter()
            .map(|(name, ty)| (name.as_str(), ty.clone()))
            .collect::<Vec<_>>();
        let output = (rng.below(2) == 0).then(|| ty(rng));
        entries.push(spec_function(&format!("f{index}"), &params, output));
    }

    for index in 0..rng.below(3) {
        let fields = (0..rng.below(3))
            .map(|field| (format!("field{field}"), ty(rng)))
            .collect::<Vec<_>>();
        let fields = fields
            .iter()
            .map(|(name, ty)| (name.as_str(), ty.clone()))
            .collect::<Vec<_>>();
        entries.push(spec_struct(&format!("S{index}"), &fields));
    }

    for index in 0..rng.below(3) {
        let cases = (0..1 + rng.below(3))
            .map(|case| (format!("Case{case}"), case as u32))
            .collect::<Vec<_>>();
        let cases = cases
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect::<Vec<_>>();
        entries.push(spec_enum(&format!("E{index}"), &cases));
    }

    for index in 0..rng.below(2) {
        let cases = (0..1 + rng.below(3))
            .map(|case| (format!("Case{case}"), case as u32))
            .collect::<Vec<_>>();
        let cases = cases
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect::<Vec<_>>();
        entries.push(spec_error_enum(&format!("Err{index}"), &cases));
    }

    for index in 0..rng.below(2) {
        let cases = (0..1 + rng.below(3))
            .map(|case| {
                let is_tuple = rng.below(2) == 0;
                (
                    format!("Case{case}"),
                    is_tuple.then(|| vec![ty(rng), ty(rng)]),
                )
            })
            .collect::<Vec<_>>();
        let cases = cases
            .iter()
            .map(|(name, types)| (name.as_str(), types.clone()))
            .collect::<Vec<_>>();
        entries.push(spec_union(&format!("U{index}"), &cases));
    }

    for index in 0..rng.below(2) {
        let params = (0..rng.below(3))
            .map(|param| {
                (
                    format!("p{param}"),
                    ty(rng),
                    ScSpecEventParamLocationV0::Data,
                )
            })
            .collect::<Vec<_>>();
        let params = params
            .iter()
            .map(|(name, ty, location)| (name.as_str(), ty.clone(), *location))
            .collect::<Vec<_>>();
        entries.push(spec_event(&format!("evt{index}"), &["transfer"], &params));
    }

    entries
}

fn shuffle(rng: &mut Rng, entries: &mut [ScSpecEntry]) {
    for index in (1..entries.len()).rev() {
        let other = rng.below(index + 1);
        entries.swap(index, other);
    }
}

/// Return the canonical interface for `entries`.
fn canonical(entries: &[ScSpecEntry]) -> ContractInterface {
    ContractInterface::from_spec(normalize(entries.to_vec()))
}

fn check(entries: &[ScSpecEntry], permutation_seed: u64) -> Result<(), String> {
    let mut permuted = entries.to_vec();
    shuffle(&mut Rng(permutation_seed), &mut permuted);

    let original = canonical(entries);
    let reordered = canonical(&permuted);
    if original.fingerprint().unwrap() != reordered.fingerprint().unwrap() {
        return Err("permuting declaration order changed the fingerprint".to_string());
    }

    let mut once = original.clone();
    once.canonicalize();
    if once != original {
        return Err("canonicalize was not idempotent".to_string());
    }
    if once.fingerprint().unwrap() != original.fingerprint().unwrap() {
        return Err("re-canonicalizing changed the fingerprint".to_string());
    }
    Ok(())
}

/// Greedily remove entries while `fails` still holds, yielding a small case.
fn shrink(
    mut entries: Vec<ScSpecEntry>,
    fails: impl Fn(&[ScSpecEntry]) -> bool,
) -> Vec<ScSpecEntry> {
    loop {
        let mut reduced = false;
        for index in 0..entries.len() {
            let mut candidate = entries.clone();
            candidate.remove(index);
            if fails(&candidate) {
                entries = candidate;
                reduced = true;
                break;
            }
        }
        if !reduced {
            return entries;
        }
    }
}

#[test]
fn generated_interfaces_are_order_independent_and_idempotent() {
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    for _ in 0..64 {
        let entries = generate(&mut rng);
        let permutation_seed = rng.next();
        if let Err(message) = check(&entries, permutation_seed) {
            let minimal = shrink(entries.clone(), |candidate| {
                check(candidate, permutation_seed).is_err()
            });
            panic!(
                "{message}; shrank from {} to {} entries: {minimal:?}",
                entries.len(),
                minimal.len()
            );
        }
    }
}

#[test]
fn shrinker_reduces_a_failing_case_to_a_minimum() {
    let mut rng = Rng(7);
    let entries = generate(&mut rng);
    let minimal = shrink(entries, |candidate| candidate.len() >= 2);
    assert_eq!(minimal.len(), 2);
}
