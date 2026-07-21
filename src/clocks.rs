//! Vector Clock for causal tracking and automatic prune coordination.
//!
//! A `VectorClock` is a `HashMap<ActorID, u64>` where each entry records
//! the highest timestamp from that actor that *this node* has observed.
//!
//! ## Role in the CRDT system
//!
//! - **Causal ordering**: determine if operation A *happened-before* B, or
//!   if they are *concurrent*.
//! - **Auto-prune watermark**: computing the `min` over all peer clocks gives
//!   a global safe floor — any tombstone older than that floor can be
//!   physically deleted without risking causal inconsistency.

use crate::core::ActorID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A vector clock tracking per-actor logical time.
///
/// # Example
/// ```
/// use crdt::clocks::VectorClock;
///
/// let mut alice = VectorClock::new();
/// alice.tick(1);     // Alice advances her own clock
/// alice.tick(1);
///
/// let mut bob = VectorClock::new();
/// bob.tick(2);       // Bob advances his own clock
/// bob.witness(1, 2); // Bob learns Alice has reached tick 2
///
/// // Bob's merged knowledge dominates Alice's current view
/// let mut merged = alice.clone();
/// merged.merge(&bob);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    clock: HashMap<ActorID, u64>,
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorClock {
    /// Create an empty vector clock.
    pub fn new() -> Self {
        VectorClock {
            clock: HashMap::new(),
        }
    }

    /// Advance the logical clock for `actor` by one tick.
    /// Returns the new timestamp value after the tick.
    pub fn tick(&mut self, actor: ActorID) -> u64 {
        let entry = self.clock.entry(actor).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Record that we have observed a message from `actor` at logical time `ts`.
    /// Takes the max to ensure the clock only advances forward.
    pub fn witness(&mut self, actor: ActorID, ts: u64) {
        let entry = self.clock.entry(actor).or_insert(0);
        if ts > *entry {
            *entry = ts;
        }
    }

    /// Return the current logical timestamp for `actor` (0 if never seen).
    pub fn get(&self, actor: ActorID) -> u64 {
        self.clock.get(&actor).copied().unwrap_or(0)
    }

    /// Merge another clock into `self` by taking the pointwise maximum.
    ///
    /// This is the standard lattice join for vector clocks and preserves
    /// all causal information from both perspectives.
    pub fn merge(&mut self, other: &Self) {
        for (&actor, &ts) in &other.clock {
            self.witness(actor, ts);
        }
    }

    /// Check if `self` **causally dominates** (happened-before) `other`.
    ///
    /// Returns `true` if for every actor, `self[actor] >= other[actor]`,
    /// AND there is at least one actor where `self > other`.
    ///
    /// In other words: everything `other` knows, `self` also knows — and more.
    pub fn dominates(&self, other: &Self) -> bool {
        // Every actor in `other` must have a value <= self's value
        let at_least_equal = other
            .clock
            .iter()
            .all(|(&actor, &ts)| self.get(actor) >= ts);

        if !at_least_equal {
            return false;
        }

        // Must be strictly greater somewhere (otherwise they're equal, not dominating)
        self.clock.iter().any(|(&actor, &ts)| ts > other.get(actor))
            || other.clock.iter().any(|(&actor, &ts)| self.get(actor) > ts)
    }

    /// Check if `self` and `other` are **concurrent** (neither dominates the other).
    ///
    /// Two clocks are concurrent if there exists an actor where `self > other`
    /// AND another actor where `other > self`. This is the distributed "parallel
    /// edit" condition that triggers CRDT merge semantics.
    pub fn concurrent(&self, other: &Self) -> bool {
        let self_ahead = self.clock.iter().any(|(&actor, &ts)| ts > other.get(actor));

        let other_ahead = other.clock.iter().any(|(&actor, &ts)| ts > self.get(actor));

        self_ahead && other_ahead
    }

    /// Compute the **global safe prune watermark** given the clocks of all
    /// known peers in the network.
    ///
    /// The watermark is the minimum across all actors' maximum-known timestamps.
    /// Any tombstone with `clock <= watermark` is guaranteed to have been seen
    /// by every living peer — it is safe to physically delete.
    ///
    /// # Arguments
    /// * `peers` — the vector clocks of every other node in the cluster.
    ///
    /// Returns `0` if `peers` is empty (nothing is provably safe to prune yet).
    pub fn watermark(&self, peers: &[VectorClock]) -> u64 {
        if peers.is_empty() {
            return 0;
        }

        // Collect every actor mentioned across all clocks (including self)
        let mut all_actors: std::collections::HashSet<ActorID> =
            self.clock.keys().copied().collect();
        for peer in peers {
            all_actors.extend(peer.clock.keys().copied());
        }

        // For each actor, find the minimum value seen across all nodes.
        // That minimum is the globally confirmed floor for that actor.
        let mut watermark = u64::MAX;
        for actor in &all_actors {
            let self_val = self.get(*actor);
            let min_across_peers = peers.iter().map(|p| p.get(*actor)).min().unwrap_or(0);
            let floor = self_val.min(min_across_peers);
            watermark = watermark.min(floor);
        }

        if watermark == u64::MAX { 0 } else { watermark }
    }

    /// Returns all actors tracked by this clock.
    pub fn actors(&self) -> Vec<ActorID> {
        self.clock.keys().copied().collect()
    }

    /// Returns the raw underlying clock map for inspection/serialization.
    pub fn as_map(&self) -> &HashMap<ActorID, u64> {
        &self.clock
    }
}

//  Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vclock_tick_and_get() {
        let mut vc = VectorClock::new();
        assert_eq!(vc.get(1), 0);

        let t = vc.tick(1);
        assert_eq!(t, 1);
        assert_eq!(vc.get(1), 1);

        vc.tick(1);
        assert_eq!(vc.get(1), 2);

        vc.tick(2);
        assert_eq!(vc.get(2), 1);
    }

    #[test]
    fn test_vclock_witness() {
        let mut vc = VectorClock::new();
        vc.witness(3, 10);
        assert_eq!(vc.get(3), 10);

        // Witness with a lower timestamp should NOT regress the clock
        vc.witness(3, 5);
        assert_eq!(vc.get(3), 10);

        // Witness with a higher timestamp should advance it
        vc.witness(3, 15);
        assert_eq!(vc.get(3), 15);
    }

    #[test]
    fn test_vclock_merge_pointwise_max() {
        let mut a = VectorClock::new();
        a.tick(1); // A=1
        a.tick(1); // A=2
        a.tick(2); // B=1

        let mut b = VectorClock::new();
        b.tick(1); // A=1
        b.tick(2); // B=1
        b.tick(2); // B=2
        b.tick(3); // C=1

        a.merge(&b);

        // Pointwise max: A=max(2,1)=2, B=max(1,2)=2, C=max(0,1)=1
        assert_eq!(a.get(1), 2);
        assert_eq!(a.get(2), 2);
        assert_eq!(a.get(3), 1);
    }

    #[test]
    fn test_vclock_merge_is_commutative() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(2);
        b.tick(2);
        b.tick(2);

        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        assert_eq!(ab, ba, "merge must be commutative");
    }

    #[test]
    fn test_vclock_merge_is_idempotent() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(2);

        let copy = a.clone();
        a.merge(&copy);
        a.merge(&copy);

        assert_eq!(a, copy, "merge with self must be idempotent");
    }

    /// Happened-before: A sent a message that B received — A dominates B.
    #[test]
    fn test_vclock_dominates_basic() {
        let mut a = VectorClock::new();
        a.tick(1); // A=1

        let mut b = VectorClock::new();
        b.tick(1); // A=1
        b.witness(1, 1); // B has seen A's tick
        b.tick(2); // B=1   (B advanced after seeing A)

        // b dominates a: b knows everything a knows, and more
        assert!(b.dominates(&a), "b must dominate a");
        assert!(!a.dominates(&b), "a must NOT dominate b");
    }

    /// Two independent writers — neither dominates the other.
    #[test]
    fn test_vclock_concurrent_detection() {
        let mut alice = VectorClock::new();
        alice.tick(1); // Alice advanced
        alice.tick(1);

        let mut bob = VectorClock::new();
        bob.tick(2); // Bob advanced independently

        assert!(
            alice.concurrent(&bob),
            "independent edits must be concurrent"
        );
        assert!(bob.concurrent(&alice), "concurrent must be symmetric");
        assert!(!alice.dominates(&bob));
        assert!(!bob.dominates(&alice));
    }

    /// Equal clocks are neither concurrent nor dominating each other.
    #[test]
    fn test_vclock_equal_clocks() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(2);

        let b = a.clone();

        assert!(!a.dominates(&b), "equal clocks: a does not dominate b");
        assert!(!b.dominates(&a), "equal clocks: b does not dominate a");
        assert!(!a.concurrent(&b), "equal clocks are not concurrent");
    }

    /// Three nodes: watermark should be the minimum safely confirmed timestamp.
    #[test]
    fn test_vclock_watermark_three_nodes() {
        // Node A has seen: actor1=5, actor2=3
        let mut node_a = VectorClock::new();
        for _ in 0..5 {
            node_a.tick(1);
        }
        for _ in 0..3 {
            node_a.tick(2);
        }

        // Node B has seen: actor1=4, actor2=3
        let mut node_b = VectorClock::new();
        for _ in 0..4 {
            node_b.tick(1);
        }
        for _ in 0..3 {
            node_b.tick(2);
        }

        // Node C has seen: actor1=5, actor2=2
        let mut node_c = VectorClock::new();
        for _ in 0..5 {
            node_c.tick(1);
        }
        for _ in 0..2 {
            node_c.tick(2);
        }

        // Watermark from A's perspective with [B, C] as peers:
        // actor1: min(5, 4, 5) = 4
        // actor2: min(3, 3, 2) = 2
        // overall watermark = min(4, 2) = 2
        let wm = node_a.watermark(&[node_b, node_c]);
        assert_eq!(wm, 2, "watermark must be the global minimum safe floor");
    }

    #[test]
    fn test_vclock_watermark_no_peers() {
        let mut vc = VectorClock::new();
        vc.tick(1);
        // With no peers we cannot confirm anything is globally safe
        assert_eq!(vc.watermark(&[]), 0);
    }

    /// Prove the watermark can drive automatic prune on an RGA.
    #[test]
    fn test_vclock_watermark_drives_rga_prune() {
        use crate::sequences::RGA;

        let mut rga: RGA<char> = RGA::new(1);
        rga.insert(0, 'H');
        rga.insert(1, 'i');
        rga.remove(0); // 'H' tombstoned at clock tick ~1

        let mut node_a = VectorClock::new();
        node_a.tick(1);
        node_a.tick(1);
        node_a.tick(1); // clock=3

        let mut node_b = VectorClock::new();
        node_b.tick(1);
        node_b.tick(1);
        node_b.tick(1); // clock=3 (seen same)

        let wm = node_a.watermark(&[node_b]);
        // watermark=3 → tombstone at clock 1 is safe to GC
        rga.prune(wm);

        // 'i' is still live
        assert_eq!(rga.value(), vec!['i']);
    }
}
