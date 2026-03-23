//! Core CRDT traits and types

pub type ActorID = u64;

/// CRDT trait
///
/// Every CRDT must:
/// - Be Clone (for merging)
/// - Be PartialEq (for convergence testing)
/// - Implement merge() with semilattice properties
pub trait Crdt: Clone + PartialEq {
    fn merge(&mut self, other: &Self);
}
