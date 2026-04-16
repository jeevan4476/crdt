//! Set CRDTs
//!
//! This module implements various Set CRDTs with different semantics
//! for handling concurrent add/remove operations.

use crate::core::Crdt;
use std::collections::HashSet;
use std::hash::Hash;

/// G-Set: Grow-only Set
///
/// The simplest Set CRDT. Elements can only be added, never removed.
/// Perfect for: vote counting, "like" buttons, permanent membership lists.
#[derive(Clone, Debug)]
pub struct GSet<T: Clone + Eq + Hash> {
    elements: HashSet<T>,
}

impl<T: Clone + Eq + Hash> PartialEq for GSet<T> {
    fn eq(&self, others: &Self) -> bool {
        self.elements == others.elements
    }
}

impl<T: Clone + Eq + Hash> Eq for GSet<T> {}

impl<T: Clone + Eq + Hash> GSet<T> {
    pub fn new() -> Self {
        GSet {
            elements: HashSet::new(),
        }
    }

    pub fn add(&mut self, elements: T) {
        self.elements.insert(elements);
    }

    pub fn contains(&self, elements: &T) -> bool {
        self.elements.contains(elements)
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }
}

impl<T: Clone + Eq + Hash> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Eq + Hash> Crdt for GSet<T> {
    fn merge(&mut self, other: &Self) {
        for element in &other.elements {
            self.elements.insert(element.clone());
        }
    }
}

/// 2P-Set: Two-Phase Set
///
/// Supports add and remove, but an element can never be added again
/// once it's been removed (tombstone is permanent).
///
/// Uses two G-Sets:
/// - `added`: Elements that have been added
/// - `removed`: Elements that have been removed (tombstones)
///
/// An element is in the set if: added - removed
///
/// **Concurrent add/remove:** Remove wins
#[derive(Clone, Debug)]
pub struct TwoPSet<T: Clone + Eq + Hash> {
    added: HashSet<T>,
    removed: HashSet<T>,
}

impl<T: Clone + Eq + Hash> PartialEq for TwoPSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.added == other.added && self.removed == other.removed
    }
}

impl<T: Clone + Eq + Hash> Eq for TwoPSet<T> {}

impl<T: Clone + Eq + Hash> TwoPSet<T> {
    pub fn new() -> Self {
        TwoPSet {
            added: HashSet::new(),
            removed: HashSet::new(),
        }
    }
    pub fn add(&mut self, element: T) {
        if !self.removed.contains(&element) {
            self.added.insert(element);
        }
    }
    pub fn remove(&mut self, element: T) {
        if self.added.contains(&element) {
            self.removed.insert(element);
        }
    }
    pub fn contains(&self, element: &T) -> bool {
        self.added.contains(element) && !self.removed.contains(element)
    }

    pub fn elements(&self) -> Vec<T> {
        self.added
            .iter()
            .filter(|e| !self.removed.contains(e))
            .cloned()
            .collect()
    }
    pub fn len(&self) -> usize {
        self.elements().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl<T: Clone + Eq + Hash> Default for TwoPSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Eq + Hash> Crdt for TwoPSet<T> {
    fn merge(&mut self, other: &Self) {
        for element in &other.added {
            self.added.insert(element.clone());
        }
        for element in &other.removed {
            self.removed.insert(element.clone());
        }
    }
}

/// OR-Set: Observed-Remove Set
///
/// The most sophisticated Set CRDT with clean, intuitive semantics.
///
/// **Key properties:**
/// - Elements can be added and removed multiple times
/// - Concurrent add/remove: **Add wins** (most intuitive)
/// - Uses unique tags to track individual add operations
///
/// **How it works:**
/// - Each `add(e)` creates a unique (element, tag) pair
/// - `remove(e)` removes only the tags observed at source
/// - Concurrent add creates new tag not observed by remove
#[derive(Clone, Debug)]
pub struct ORSet<T: Clone + Eq + Hash> {
    actor: crate::core::ActorID,
    added: std::collections::HashMap<T, HashSet<(crate::core::ActorID, u64)>>,
    removed: std::collections::HashMap<T, HashSet<(crate::core::ActorID, u64)>>,
    next_uid: u64,
}

impl<T: Clone + Eq + Hash> PartialEq for ORSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.added == other.added && self.removed == other.removed
    }
}

impl<T: Clone + Eq + Hash> Eq for ORSet<T> {}

impl<T: Clone + Eq + Hash> ORSet<T> {
    pub fn new(actor: crate::core::ActorID) -> Self {
        ORSet {
            actor,
            added: std::collections::HashMap::new(),
            removed: std::collections::HashMap::new(),
            next_uid: 0,
        }
    }

    pub fn add(&mut self, element: T) {
        let tag = self.generate_tag();
        self.added.entry(element).or_default().insert(tag);
    }

    pub fn remove(&mut self, element: &T) {
        if let Some(active_tags) = self.added.get(element) {
            let removed_for_element = self.removed.entry(element.clone()).or_default();
            for tag in active_tags {
                removed_for_element.insert(*tag);
            }
        }
    }

    pub fn contains(&self, element: &T) -> bool {
        if let Some(active_tags) = self.added.get(element) {
            let removed_tags = self.removed.get(element);
            for tag in active_tags {
                if removed_tags.is_none_or(|r| !r.contains(tag)) {
                    return true;
                }
            }
        }
        false
    }

    pub fn elements(&self) -> Vec<T> {
        self.added
            .keys()
            .filter(|&k| self.contains(k))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.added.keys().filter(|&k| self.contains(k)).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn generate_tag(&mut self) -> (crate::core::ActorID, u64) {
        let uid = self.next_uid;
        self.next_uid += 1;
        (self.actor, uid)
    }
}

impl<T: Clone + Eq + Hash> Crdt for ORSet<T> {
    fn merge(&mut self, other: &Self) {
        for (element, other_tags) in &other.added {
            self.added
                .entry(element.clone())
                .or_default()
                .extend(other_tags.iter().cloned());
        }
        for (element, other_tags) in &other.removed {
            self.removed
                .entry(element.clone())
                .or_default()
                .extend(other_tags.iter().cloned());
        }
        self.next_uid = self.next_uid.max(other.next_uid);
    }
}

/// LWW-Set: Last-Write-Wins Element Set
///
/// Uses timestamps to resolve conflicts. The operation with the
/// highest timestamp wins.
///
/// **Key properties:**
/// - Elements can be added and removed multiple times
/// - Concurrent add/remove: **Highest timestamp wins**
/// - Requires synchronized clocks (or Lamport timestamps)
///
/// **Comparison with OR-Set:**
/// - OR-Set: Add always wins (uses unique tags)
/// - LWW-Set: Latest timestamp wins (uses timestamps)
///
/// **Use cases:** Replicated databases, configuration management,
/// any scenario where "last edit wins" is desired.
#[derive(Clone, Debug)]
pub struct LWWSet<T: Clone + Eq + Hash> {
    actor: crate::core::ActorID,
    added: std::collections::HashMap<T, (u64, crate::core::ActorID)>,
    removed: std::collections::HashMap<T, (u64, crate::core::ActorID)>,
    clock: u64,
}

impl<T: Clone + Eq + Hash> PartialEq for LWWSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.added == other.added && self.removed == other.removed
    }
}

impl<T: Clone + Eq + Hash> Eq for LWWSet<T> {}

impl<T: Clone + Eq + Hash> LWWSet<T> {
    pub fn new(actor: crate::core::ActorID) -> Self {
        LWWSet {
            actor,
            added: std::collections::HashMap::new(),
            removed: std::collections::HashMap::new(),
            clock: 0,
        }
    }

    pub fn add(&mut self, element: T) {
        let timestamp = self.tick();
        self.added.insert(element, timestamp);
    }

    pub fn remove(&mut self, element: T) {
        let timestamp = self.tick();
        self.removed.insert(element, timestamp);
    }

    pub fn contains(&self, element: &T) -> bool {
        let t_add = self.added.get(element);
        let t_rem = self.removed.get(element);

        match (t_add, t_rem) {
            (Some(ta), Some(tr)) => ta > tr, // Uses Tuple automatic comparison (clock, actor)
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => false,
        }
    }

    pub fn elements(&self) -> Vec<T> {
        self.added
            .keys()
            .filter(|&k| self.contains(k))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.added.keys().filter(|&k| self.contains(k)).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn tick(&mut self) -> (u64, crate::core::ActorID) {
        self.clock += 1;
        (self.clock, self.actor)
    }
}

impl<T: Clone + Eq + Hash> Crdt for LWWSet<T> {
    fn merge(&mut self, other: &Self) {
        for (element, &other_ts) in &other.added {
            let entry = self.added.entry(element.clone()).or_insert(other_ts);
            if other_ts > *entry {
                *entry = other_ts;
            }
        }
        for (element, &other_ts) in &other.removed {
            let entry = self.removed.entry(element.clone()).or_insert(other_ts);
            if other_ts > *entry {
                *entry = other_ts;
            }
        }
        self.clock = self.clock.max(other.clock);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gset_add() {
        let mut set = GSet::new();

        set.add("alice");
        set.add("bob");

        assert!(set.contains(&"alice"));
        assert!(set.contains(&"bob"));
        assert!(!set.contains(&"charlie"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_gset_convergence() {
        let mut s1 = GSet::new();
        let mut s2 = GSet::new();

        s1.add("alice");
        s1.add("bob");

        s2.add("bob");
        s2.add("charlie");

        // Merge both ways
        s1.merge(&s2);
        s2.merge(&s1);

        // Should converge
        assert_eq!(s1.len(), 3);
        assert_eq!(s2.len(), 3);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_gset_idempotent() {
        let mut s1 = GSet::new();
        s1.add("alice");

        let s2 = s1.clone();

        s1.merge(&s2);
        s1.merge(&s2);

        assert_eq!(s1.len(), 1);
        assert!(s1.contains(&"alice"));
    }
}

#[cfg(test)]
mod twop_tests {
    use super::*;

    #[test]
    fn test_2pset_add_remove() {
        let mut set = TwoPSet::new();

        set.add("alice");
        assert!(set.contains(&"alice"));

        set.remove("alice");
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_2pset_cannot_readd() {
        let mut set = TwoPSet::new();

        set.add("alice");
        set.remove("alice");
        set.add("alice"); // Should have no effect

        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_2pset_concurrent_add_remove() {
        let mut s1 = TwoPSet::new();
        let mut s2 = TwoPSet::new();

        // Concurrent: s1 adds, s2 removes
        s1.add("alice");
        s2.remove("alice"); // Actually does nothing (not in s2's added set)

        // But let's test proper concurrent scenario:
        s1.add("bob");
        s2.add("bob");
        s1.remove("bob"); // s1 removes bob

        s1.merge(&s2);
        s2.merge(&s1);

        // Remove wins
        assert!(!s1.contains(&"bob"));
        assert!(!s2.contains(&"bob"));
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_2pset_convergence() {
        let mut s1 = TwoPSet::new();
        let mut s2 = TwoPSet::new();

        s1.add("alice");
        s1.add("bob");

        s2.add("bob");
        s2.add("charlie");
        s2.remove("charlie");

        s1.merge(&s2);
        s2.merge(&s1);

        assert_eq!(s1, s2);
        assert!(s1.contains(&"alice"));
        assert!(s1.contains(&"bob"));
        assert!(!s1.contains(&"charlie"));
    }
}

#[cfg(test)]
mod or_tests {
    use super::*;

    #[test]
    fn test_orset_add_remove() {
        let mut set = ORSet::new(1);

        set.add("alice");
        assert!(set.contains(&"alice"));

        set.remove(&"alice");
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_orset_can_readd() {
        let mut set = ORSet::new(1);

        set.add("alice");
        set.remove(&"alice");
        set.add("alice");

        assert!(set.contains(&"alice"));
    }

    #[test]
    fn test_orset_concurrent_add_wins() {
        let mut s1 = ORSet::new(1);
        let mut s2 = ORSet::new(2);

        // Both add "alice" (different tags)
        s1.add("alice"); // tag: (1, 0)
        s2.add("alice"); // tag: (2, 0)

        // s1 removes "alice" (only removes its own tag)
        s1.remove(&"alice");

        // Merge
        s1.merge(&s2);
        s2.merge(&s1);

        // Add wins: alice is still in the set
        assert!(s1.contains(&"alice"));
        assert!(s2.contains(&"alice"));
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_orset_remove_then_add() {
        let mut s1 = ORSet::new(1);
        let mut s2 = ORSet::new(2);

        // s1: add then remove
        s1.add("alice");
        s1.remove(&"alice");

        // s2: add after s1's remove (concurrent)
        s2.add("alice");

        // Merge
        s1.merge(&s2);
        s2.merge(&s1);

        // alice should be in the set (add wins)
        assert!(s1.contains(&"alice"));
        assert!(s2.contains(&"alice"));
    }

    #[test]
    fn test_orset_multiple_adds_same_element() {
        let mut set = ORSet::new(1);

        set.add("alice");
        set.add("alice"); // Add again
        set.add("alice"); // And again

        // Should have 3 different tags for alice
        assert!(set.contains(&"alice"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_orset_convergence() {
        let mut s1 = ORSet::new(1);
        let mut s2 = ORSet::new(2);

        // Complex scenario
        s1.add("alice");
        s1.add("bob");

        s2.add("bob");
        s2.add("charlie");

        s1.remove(&"bob"); // s1 removes bob

        s2.add("bob"); // s2 adds bob again (concurrent)

        // Merge both ways
        s1.merge(&s2);
        s2.merge(&s1);

        // Should converge
        assert_eq!(s1, s2);

        // alice: in s1 only
        assert!(s1.contains(&"alice"));

        // bob: should be present (concurrent add wins)
        assert!(s1.contains(&"bob"));

        // charlie: in s2 only
        assert!(s1.contains(&"charlie"));
    }

    #[test]
    fn test_orset_idempotence() {
        let mut s1 = ORSet::new(1);
        s1.add("alice");
        s1.add("bob");

        let s2 = s1.clone();

        s1.merge(&s2);
        s1.merge(&s2);
        s1.merge(&s2);

        // Multiple merges should have no effect
        assert_eq!(s1.len(), 2);
        assert!(s1.contains(&"alice"));
        assert!(s1.contains(&"bob"));
    }
}

#[cfg(test)]
mod lww_tests {
    use super::*;

    #[test]
    fn test_lwwset_add_remove() {
        let mut set = LWWSet::new(1);

        set.add("alice");
        assert!(set.contains(&"alice"));

        set.remove("alice");
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_lwwset_last_write_wins() {
        let mut set = LWWSet::new(1);

        set.add("item"); // timestamp: (1, 1)
        set.remove("item"); // timestamp: (2, 1)

        // Remove wins (higher timestamp)
        assert!(!set.contains(&"item"));

        set.add("item"); // timestamp: (3, 1)

        // Add wins now (highest timestamp)
        assert!(set.contains(&"item"));
    }

    #[test]
    fn test_lwwset_concurrent_different_timestamps() {
        let mut s1 = LWWSet::new(1);
        let mut s2 = LWWSet::new(2); // Actor 2

        s1.add("item"); // timestamp: (1, 1)
        s2.remove("item"); // timestamp: (1, 2) (concurrent, same clock value)

        // Tie breaking mechanism: Tuple comparison (clock, actor).
        // (1, 2) > (1, 1) so actor 2 (remove) wins!

        s1.merge(&s2);
        s2.merge(&s1);

        assert!(!s1.contains(&"item")); // Remove wins on tie because 2 > 1
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_lwwset_convergence() {
        let mut s1 = LWWSet::new(1);
        let mut s2 = LWWSet::new(2);

        s1.add("alice");
        s1.add("bob");

        s2.add("charlie");
        s2.remove("bob"); // Higher clock timestamp than s1's add usually

        s1.merge(&s2);
        s2.merge(&s1);

        assert_eq!(s1, s2);
        assert!(s1.contains(&"alice"));
        assert!(s1.contains(&"charlie"));

        // bob's fate depends on timestamps
        assert!(!s1.contains(&"bob"));
    }

    #[test]
    fn test_lwwset_multiple_add_remove() {
        let mut set = LWWSet::new(1);

        set.add("item"); // t=1
        set.remove("item"); // t=2
        set.add("item"); // t=3
        set.remove("item"); // t=4
        set.add("item"); // t=5

        // Latest operation (add at t=5) wins
        assert!(set.contains(&"item"));
    }

    #[test]
    fn test_lwwset_idempotence() {
        let mut s1 = LWWSet::new(1);
        s1.add("alice");
        s1.add("bob");

        let s2 = s1.clone();

        s1.merge(&s2);
        s1.merge(&s2);
        s1.merge(&s2);

        assert_eq!(s1.len(), 2);
        assert!(s1.contains(&"alice"));
        assert!(s1.contains(&"bob"));
    }
}
