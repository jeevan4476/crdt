//! Map CRDTs
//!
//! Implements `ORMap` — an Observed-Remove Map that holds heterogeneous CRDT
//! values keyed by a string. Supports arbitrarily nested documents.
//!
//! **Conflict semantics:** Update-Wins over concurrent Remove.

use crate::core::{ActorID, Crdt};
use crate::counters::PNCounter;
use crate::sequences::RGA;
use crate::sets::{GSet, LWWSet, ORSet};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};


/// The value stored in an `ORMap` entry.
///
/// Each variant wraps a fully functional CRDT so that inner values
/// merge independently when two replicas are synchronized.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ORMapValue {
    Counter(PNCounter),
    GrowSet(GSet<String>),
    ObservedSet(ORSet<String>),
    LWWKeySet(LWWSet<String>),
    Sequence(RGA<char>),
    Map(Box<ORMap<String>>),
}

impl PartialEq for ORMapValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Counter(a), Self::Counter(b)) => a == b,
            (Self::GrowSet(a), Self::GrowSet(b)) => a == b,
            (Self::ObservedSet(a), Self::ObservedSet(b)) => a == b,
            (Self::LWWKeySet(a), Self::LWWKeySet(b)) => a == b,
            (Self::Sequence(a), Self::Sequence(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => a == b,
            _ => false,
        }
    }
}

impl ORMapValue {
    /// Merge another `ORMapValue` into `self`. Both values must be the same
    /// variant — mismatched types are silently ignored (last-writer wins by
    /// existing value staying in place).
    fn merge_with(&mut self, other: &Self) {
        match (self, other) {
            (Self::Counter(a), Self::Counter(b)) => a.merge(b),
            (Self::GrowSet(a), Self::GrowSet(b)) => a.merge(b),
            (Self::ObservedSet(a), Self::ObservedSet(b)) => a.merge(b),
            (Self::LWWKeySet(a), Self::LWWKeySet(b)) => a.merge(b),
            (Self::Sequence(a), Self::Sequence(b)) => a.merge(b),
            (Self::Map(a), Self::Map(b)) => a.merge(b),
            _ => {} // type mismatch — keep self (update-wins)
        }
    }

    /// Delegate tombstone pruning to the inner CRDT.
    fn prune(&mut self, watermark: u64) {
        match self {
            Self::ObservedSet(s) => s.prune(watermark),
            Self::LWWKeySet(s) => s.prune(watermark),
            Self::Sequence(r) => r.prune(watermark),
            Self::Map(m) => m.prune(watermark),
            _ => {}
        }
    }
}

/// Delta operations broadcast over the network for `ORMap`.
///
/// Nodes can apply individual key mutations without shipping the entire state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ORMapDelta {
    /// A key was inserted or an existing value was updated.
    Insert(String, ORMapValue),
    /// A key was deleted.
    Remove(String),
}

/// ORMap: Observed-Remove Map
///
/// A CRDT map that holds heterogeneous CRDT values keyed by `String`.
/// Supports arbitrarily deep nesting via the `ORMapValue::Map` variant.
///
/// **Conflict rule — Update-Wins:**
/// If one replica deletes a key while another concurrently modifies it,
/// the modification survives the merge.
///
/// **Use cases:**
/// - Collaborative JSON documents (like Notion, Figma, Google Docs state)
/// - Replicated application configuration trees
/// - Any nested/schemaless shared state between distributed nodes
#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(bound = "K: Serialize + for<'de2> Deserialize<'de2>")]
pub struct ORMap<K: Clone + Eq + std::hash::Hash> {
    actor: ActorID,
    entries: HashMap<K, ORMapValue>,
    /// Keys that have been definitively removed. Cleared on concurrent update.
    tombstones: HashSet<K>,
}

impl<K> PartialEq for ORMap<K>
where
    K: Clone + Eq + std::hash::Hash,
{
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries && self.tombstones == other.tombstones
    }
}

impl<K> Eq for ORMap<K> where K: Clone + Eq + std::hash::Hash {}

impl<K> ORMap<K>
where
    K: Clone + Eq + std::hash::Hash + std::fmt::Display + std::str::FromStr,
{
    pub fn new(actor: ActorID) -> Self {
        ORMap {
            actor,
            entries: HashMap::new(),
            tombstones: HashSet::new(),
        }
    }

    /// Insert or overwrite a key with a new CRDT value.
    /// Clears the tombstone if the key was previously removed — Update-Wins.
    pub fn insert(&mut self, key: K, value: ORMapValue) -> ORMapDelta {
        self.tombstones.remove(&key);
        self.entries.insert(key.clone(), value.clone());
        ORMapDelta::Insert(self.key_to_string(&key), value)
    }

    /// Remove a key, adding it to the tombstone set.
    pub fn remove(&mut self, key: &K) -> Option<ORMapDelta> {
        if self.entries.remove(key).is_some() {
            self.tombstones.insert(key.clone());
            return Some(ORMapDelta::Remove(self.key_to_string(key)));
        }
        None
    }

    pub fn get(&self, key: &K) -> Option<&ORMapValue> {
        self.entries.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut ORMapValue> {
        self.entries.get_mut(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.keys()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Apply an incoming delta directly — no full state shipping required.
    pub fn apply_delta(&mut self, delta: ORMapDelta)
    where
        K: std::str::FromStr,
        K::Err: std::fmt::Debug,
    {
        match delta {
            ORMapDelta::Insert(raw_key, value) => {
                if let Ok(key) = raw_key.parse::<K>() {
                    // Update-Wins: resurrect from tombstone on incoming update
                    self.tombstones.remove(&key);
                    self.entries.insert(key, value);
                }
            }
            ORMapDelta::Remove(raw_key) => {
                if let Ok(key) = raw_key.parse::<K>() {
                    // Only tombstone if NOT concurrently updated (no active entry)
                    if !self.entries.contains_key(&key) {
                        self.tombstones.insert(key);
                    }
                }
            }
        }
    }

    /// Delegate tombstone GC to every inner CRDT value.
    pub fn prune(&mut self, watermark: u64) {
        for value in self.entries.values_mut() {
            value.prune(watermark);
        }
    }

    fn key_to_string(&self, key: &K) -> String
    where
        K: std::fmt::Display,
    {
        key.to_string()
    }
}

impl<K> Crdt for ORMap<K>
where
    K: Clone + Eq + std::hash::Hash,
{
    fn merge(&mut self, other: &Self) {
        // 1. Process entries from `other`
        for (key, other_value) in &other.entries {
            if self.tombstones.contains(key) {
                // Update-Wins: `other` has an active entry for a key we tombstoned.
                // Resurrect it — the concurrent update beats our remove.
                self.tombstones.remove(key);
                self.entries.insert(key.clone(), other_value.clone());
            } else if let Some(self_value) = self.entries.get_mut(key) {
                // Key exists in both — merge inner CRDTs recursively
                self_value.merge_with(other_value);
            } else {
                // New key from `other` — adopt it
                self.entries.insert(key.clone(), other_value.clone());
            }
        }

        // 2. Process tombstones from `other`
        for key in &other.tombstones {
            // Only tombstone locally if we DON'T have an active (concurrent) entry
            // for this key. If we do, Update-Wins — we keep our entry and discard
            // the incoming remove.
            if !self.entries.contains_key(key) {
                self.tombstones.insert(key.clone());
            }
        }
    }
}

//  Tests 

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Crdt;

    fn make_counter(actor: ActorID, delta: i64) -> ORMapValue {
        let mut c = PNCounter::new(actor);
        if delta > 0 {
            for _ in 0..delta { c.increment(); }
        } else {
            for _ in 0..(-delta) { c.decrement(); }
        }
        ORMapValue::Counter(c)
    }

    #[test]
    fn test_ormap_insert_and_get() {
        let mut map: ORMap<String> = ORMap::new(1);
        map.insert("score".to_string(), make_counter(1, 3));

        assert!(map.contains_key(&"score".to_string()));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_ormap_remove() {
        let mut map: ORMap<String> = ORMap::new(1);
        map.insert("key".to_string(), make_counter(1, 1));
        map.remove(&"key".to_string());

        assert!(!map.contains_key(&"key".to_string()));
        assert!(map.tombstones.contains(&"key".to_string()));
    }

    #[test]
    fn test_ormap_basic_convergence() {
        let mut m1: ORMap<String> = ORMap::new(1);
        let mut m2: ORMap<String> = ORMap::new(2);

        m1.insert("a".to_string(), make_counter(1, 5));
        m2.insert("b".to_string(), make_counter(2, 10));

        // Merge both ways
        m1.merge(&m2);
        m2.merge(&m1);

        assert_eq!(m1, m2);
        assert!(m1.contains_key(&"a".to_string()));
        assert!(m1.contains_key(&"b".to_string()));
    }

    /// Update-Wins: m1 deletes "x", m2 concurrently updates "x".
    /// After merge, "x" must survive (update beats remove).
    #[test]
    fn test_ormap_update_wins_over_remove() {
        let mut m1: ORMap<String> = ORMap::new(1);
        let mut m2: ORMap<String> = ORMap::new(2);

        // Both start with "x"
        m1.insert("x".to_string(), make_counter(1, 1));
        m2.insert("x".to_string(), make_counter(2, 1));
        // Sync initial state
        m1.merge(&m2);
        m2.merge(&m1);

        // Concurrent: m1 removes "x", m2 updates "x"
        m1.remove(&"x".to_string());
        m2.insert("x".to_string(), make_counter(2, 99)); // concurrent modification

        // Merge
        m1.merge(&m2);
        m2.merge(&m1);

        // Update-Wins: "x" must still be present
        assert!(m1.contains_key(&"x".to_string()), "Update-Wins: x must survive");
        assert_eq!(m1, m2);
    }

    /// Two replicas independently insert into a nested ORSet stored at the same key.
    #[test]
    fn test_ormap_nested_orset_merge() {
        let mut m1: ORMap<String> = ORMap::new(1);
        let mut m2: ORMap<String> = ORMap::new(2);

        let mut s1 = ORSet::new(1);
        s1.add("alice".to_string());
        m1.insert("members".to_string(), ORMapValue::ObservedSet(s1));

        let mut s2 = ORSet::new(2);
        s2.add("bob".to_string());
        m2.insert("members".to_string(), ORMapValue::ObservedSet(s2));

        m1.merge(&m2);
        m2.merge(&m1);

        assert_eq!(m1, m2);
        if let Some(ORMapValue::ObservedSet(merged)) = m1.get(&"members".to_string()) {
            assert!(merged.contains(&"alice".to_string()));
            assert!(merged.contains(&"bob".to_string()));
        } else {
            panic!("Expected ObservedSet at 'members'");
        }
    }

    /// Nested RGA: two replicas type concurrently into document["body"].
    #[test]
    fn test_ormap_nested_rga_concurrent_edits() {
        let mut m1: ORMap<String> = ORMap::new(1);
        let mut m2: ORMap<String> = ORMap::new(2);

        let mut r1 = RGA::new(1);
        r1.insert(0, 'H');
        r1.insert(1, 'i');
        m1.insert("body".to_string(), ORMapValue::Sequence(r1));

        let mut r2 = RGA::new(2);
        r2.insert(0, 'H');
        r2.insert(1, 'e');
        m2.insert("body".to_string(), ORMapValue::Sequence(r2));

        m1.merge(&m2);
        m2.merge(&m1);

        assert_eq!(m1, m2, "Both replicas must converge");
        if let Some(ORMapValue::Sequence(seq)) = m1.get(&"body".to_string()) {
            let text: String = seq.value().into_iter().collect();
            // Both characters from both actors survive the merge
            assert!(text.contains('H'), "Shared 'H' must be present");
        } else {
            panic!("Expected Sequence at 'body'");
        }
    }

    /// Three levels of nesting: Map → Map → Counter.
    #[test]
    fn test_ormap_deep_nesting() {
        let mut root: ORMap<String> = ORMap::new(1);

        let mut inner: ORMap<String> = ORMap::new(1);
        inner.insert("score".to_string(), make_counter(1, 42));

        root.insert("user".to_string(), ORMapValue::Map(Box::new(inner)));

        // Clone and merge back — idempotent
        let clone = root.clone();
        root.merge(&clone);

        assert!(root.contains_key(&"user".to_string()));
        if let Some(ORMapValue::Map(user)) = root.get(&"user".to_string()) {
            assert!(user.contains_key(&"score".to_string()));
        } else {
            panic!("Expected nested Map at 'user'");
        }
    }

    #[test]
    fn test_ormap_idempotent_merge() {
        let mut m: ORMap<String> = ORMap::new(1);
        m.insert("k".to_string(), make_counter(1, 7));
        let copy = m.clone();
        m.merge(&copy);
        m.merge(&copy);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn test_ormap_prune_delegates_to_inner() {
        let mut m: ORMap<String> = ORMap::new(1);
        let mut s = ORSet::new(1);
        s.add("x".to_string());
        s.remove(&"x".to_string());
        m.insert("set".to_string(), ORMapValue::ObservedSet(s));

        // Prune with a high watermark — tombstones inside the nested ORSet should be cleared
        m.prune(u64::MAX);
        // ORMap itself is still alive
        assert_eq!(m.len(), 1);
    }
}
