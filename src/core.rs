//! Core CRDT traits and types

pub type ActorID = u64;

/// CRDT trait
///
/// Every CRDT must:
/// - Be Clone (for merging)
/// - Be PartialEq (for convergence testing)
/// - Implement merge() with semilattice properties
/// - Be Serialize and Deserialize (for network broadcasting)
pub trait Crdt: Clone + PartialEq {
    fn merge(&mut self, other: &Self);
}

/// Trait for CRDT types that can apply operational deltas to update state.
pub trait ApplyDelta<D> {
    fn apply_delta(&mut self, delta: D);
}
