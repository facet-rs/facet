//! Value-semantic primitives for vix.
//!
//! LAYOUTS LIVE IN weavy::mem — not here. The two-authority ruling
//! (constitution A5/A6: Rust types described by facet, vix/fable types
//! owning an optimized recorded ABI) lands on weavy::mem::Descriptor,
//! which already exists, is generic over the schema authority
//! (SchemaRef), and already carries records-at-offsets, enums as
//! tag+variants, and byte-ownership proofs. A language-local layout
//! vocabulary was invented here first and deleted when weavy::mem was
//! read properly: one description vocabulary means ONE.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// `f64` under the IEEE totalOrder relation, NaN canonicalized at
/// construction so equality, ordering, and hashing agree. The language
/// deliberately trades IEEE comparison semantics for a total order
/// (invariant: every vix value is hashable AND totally ordered).
#[derive(Clone, Copy)]
pub struct TotalF64(f64);

impl TotalF64 {
    pub fn new(value: f64) -> Self {
        Self(if value.is_nan() { f64::NAN } else { value })
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl fmt::Debug for TotalF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl PartialEq for TotalF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for TotalF64 {}

impl PartialOrd for TotalF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TotalF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Hash for TotalF64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.to_bits());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_are_totally_ordered_and_hash_consistently() {
        let mut values: Vec<TotalF64> = [f64::NAN, 1.0, -0.0, 0.0, -1.0]
            .into_iter()
            .map(TotalF64::new)
            .collect();
        values.sort();
        let ordered: Vec<f64> = values.iter().map(|v| v.get()).collect();
        assert_eq!(ordered[0], -1.0);
        assert_eq!(ordered[1].to_bits(), (-0.0f64).to_bits());
        assert_eq!(ordered[2].to_bits(), 0.0f64.to_bits());
        assert_eq!(ordered[3], 1.0);
        assert!(ordered[4].is_nan());

        // NaN equals NaN after canonicalization: memo keys must agree.
        assert_eq!(TotalF64::new(f64::NAN), TotalF64::new(-f64::NAN));
    }
}
