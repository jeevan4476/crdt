//! Set CRDTs
//!
//! This module implements various Set CRDTs with different semantics
//! for handling concurrent add/remove operations.

use std::{collections::HashSet};
use std::hash::Hash;
use crate::core::{Crdt};

/// G-Set: Grow-only Set
///
/// The simplest Set CRDT. Elements can only be added, never removed.
/// Perfect for: vote counting, "like" buttons, permanent membership lists.
#[derive(Clone,Debug)]
pub struct GSet<T:Clone+Eq+Hash>{
    elements: HashSet<T>,
}

impl<T:Clone+Eq+Hash> PartialEq for GSet<T>{
    fn eq(&self,others:&Self)->bool{
        self.elements == others.elements
    }
}

impl<T:Clone+Eq+Hash> Eq for GSet<T>{}

impl<T:Clone+Eq+Hash> GSet<T>{
    pub fn new()->Self{
        GSet { elements: HashSet::new() }
    }

    pub fn add(&mut self,elements:T){
        self.elements.insert(elements);
    }

    pub fn contains(&self,elements:&T)->bool{
        self.elements.contains(elements)
    }

    pub fn is_empty(&self)->bool{
        self.elements.is_empty()
    }

    pub fn len(&self)->usize{
        self.elements.len()
    }
}

impl<T: Clone + Eq + Hash> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T:Clone+Eq+Hash> Crdt for GSet<T> {
    fn merge(&mut self, other: &Self) {
        for element in &other.elements{
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
#[derive(Clone,Debug)]
pub struct TwoPSet<T:Clone+Eq+Hash>{
    added:HashSet<T>,
    removed:HashSet<T>
}

impl<T:Clone+Eq+Hash> PartialEq for TwoPSet<T>{
    fn eq(&self, other: &Self) -> bool {
        self.added == other.added && self.removed == other.removed
    }
}

impl<T:Clone+Eq+Hash> Eq for TwoPSet<T>{}

impl<T:Clone+Eq+Hash> TwoPSet<T>{
    pub fn new()->Self{
        TwoPSet { added: HashSet::new(), removed: HashSet::new() }
    }
    pub fn add(&mut self,element:T){
        if !self.removed.contains(&element){
            self.added.insert(element);
        }
    }
    pub fn remove(&mut self,element:T){
        if self.added.contains(&element){
            self.removed.insert(element);
        }
    }
    pub fn contains(&self,element:&T)->bool{
        self.added.contains(element) && !self.removed.contains(element)
    }

    pub fn elements(&self)->Vec<T>{
        self.added.iter().filter(|e| !self.removed.contains(e)).cloned().collect()
    }
    pub fn len(&self)->usize{
        self.elements().len()
    }
    pub fn is_empty(&self)->bool{
        self.len()==0
    }
}
impl<T: Clone + Eq + Hash> Default for TwoPSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T:Clone+Eq+Hash>Crdt for TwoPSet<T>{
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
#[derive(Clone,Debug)]
pub struct ORSet<T:Clone+Eq+Hash>{
    elements: HashSet<(T,u64)>,
    next_uid: u64
}

impl<T: Clone + Eq + Hash> PartialEq for ORSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.elements == other.elements
    }
}

impl<T: Clone + Eq + Hash> Eq for ORSet<T> {}

impl<T:Clone+Eq+Hash> ORSet<T>{
    pub fn new()->Self{
        ORSet { elements: HashSet::new(), next_uid: 0 }
    }

    pub fn add(&mut self,element:T){
        let uid = self.generate_uid();
        self.elements.insert((element,uid));
    }

    /// Remove an element
    ///
    /// Removes all (element, tag) pairs for this element that are
    /// currently in the set. Concurrent adds with different tags
    /// will survive (add wins).
    pub fn remove(&mut self,element:&T){
        let to_remove : Vec<_>= self.elements.iter().filter(|(e,_)| e == element ).cloned().collect();
        for pair in to_remove{
            self.elements.remove(&pair);
        }
    }

    pub fn contains(&self,element:&T)->bool{
        self.elements.iter().any(|(e,_)| e==element)
    }

    pub fn elements(&self)->Vec<T>{
        let mut unique: HashSet<T>=HashSet::new();
        for(element,_) in &self.elements{
            unique.insert(element.clone());
        }
        unique.into_iter().collect()
    }
    pub fn len(&self) -> usize {
        self.elements().len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
    /// Generate a unique ID
    ///
    /// In a real implementation, this should be globally unique
    /// (e.g., timestamp + actor_id). For simplicity, we use a counter.
    fn generate_uid(&mut self)->u64{
        let uid = self.next_uid;
        self.next_uid+=1;
        uid
    }
}

impl<T:Clone+Eq+Hash> Default for ORSet<T>{
    fn default() -> Self {
        Self::new()
    }
}

impl<T:Clone+Eq+Hash> Crdt for ORSet<T>{
    fn merge(&mut self, other: &Self) {
        for pair in &other.elements{
            self.elements.insert(pair.clone());
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
#[derive(Clone,Debug)]
pub struct LWWSet<T:Clone+Eq+Hash>{
    added: HashSet<(T,u64)>,
    removed: HashSet<(T,u64)>,
    clock:u64  //lamport clock
}

impl<T:Clone+Eq+Hash> PartialEq for LWWSet<T>{
    fn eq(&self, other: &Self) -> bool {
        self.added == other.added && self.removed == other.removed
    }
}

impl<T:Clone+Eq+Hash> Eq for LWWSet<T>{}

impl<T:Clone+Eq+Hash> LWWSet<T>{
    pub fn new()->Self{
        LWWSet { added: HashSet::new(), removed: HashSet::new(), clock: 0 }
    }
    pub fn add(&mut self,element:T){
        let timestamp = self.tick();
        self.added.insert((element,timestamp));
    }

    pub fn remove(&mut self,element:T){
        let timestamp = self.tick();
        self.removed.insert((element,timestamp));
    }

    pub fn contains(&self,element:&T)->bool{
        let max_added = self.added.iter().filter(|(e,_)| e == element)
        .map(|(_,t)| t).max();
        let max_removed = self.removed.iter()
        .filter(|(e, _)| e == element)
        .map(|(_, t)| t)
        .max();
        
        match (max_added,max_removed) {
            (Some(t_add), Some(t_rem)) => t_add > t_rem,  // Latest wins
            (Some(_), None) => true,                       // Only added
            (None, Some(_)) => false,                      // Only removed
            (None, None) => false,                         // Never seen
        }
    }

    pub fn elements(&self)->Vec<T>{
        let mut unique: HashSet<T> = HashSet::new();

        for(element,_) in &self.added{
            unique.insert(element.clone());
        }
        unique.into_iter().filter(|e| self.contains(e)).collect()
    }

    pub fn len(&self)->usize{
        self.elements().len()
    }

    pub fn is_empty(&self)->bool{
        self.elements().is_empty()
    }
    fn tick(&mut self)->u64{
        self.clock+=1;
        self.clock
    }
}

impl<T:Clone+Eq+Hash> Default for LWWSet<T>{
    fn default() -> Self {
        Self::new()
    }
}

impl<T:Clone+Eq+Hash> Crdt for LWWSet<T>{
    fn merge(&mut self, other: &Self) {
        for pair in &other.added{
            self.added.insert(pair.clone());
        }
        for pair in &other.removed{
            self.removed.insert(pair.clone());
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
        set.add("alice");  // Should have no effect
        
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_2pset_concurrent_add_remove() {
        let mut s1 = TwoPSet::new();
        let mut s2 = TwoPSet::new();

        // Concurrent: s1 adds, s2 removes
        s1.add("alice");
        s2.remove("alice");  // Actually does nothing (not in s2's added set)

        // But let's test proper concurrent scenario:
        s1.add("bob");
        s2.add("bob");
        s1.remove("bob");  // s1 removes bob

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
        let mut set = ORSet::new();
        
        set.add("alice");
        assert!(set.contains(&"alice"));
        
        set.remove(&"alice");
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_orset_can_readd() {
        let mut set = ORSet::new();
        
        set.add("alice");
        set.remove(&"alice");
        set.add("alice");  // Can re-add! ✅
        
        assert!(set.contains(&"alice"));
    }

    #[test]
    fn test_orset_concurrent_add_wins() {
        let mut s1 = ORSet::new();
        let mut s2 = ORSet::new();

        // Both add "alice" (different tags)
        s1.add("alice");  // tag: 0
        s2.add("alice");  // tag: 0 (different replica)

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
        let mut s1 = ORSet::new();
        let mut s2 = ORSet::new();

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
        let mut set = ORSet::new();
        
        set.add("alice");
        set.add("alice");  // Add again
        set.add("alice");  // And again

        // Should have 3 different tags for alice
        let alice_tags: Vec<_> = set.elements
            .iter()
            .filter(|(e, _)| e == &"alice")
            .collect();
        
        assert_eq!(alice_tags.len(), 3);
        
        // But contains returns true (obviously)
        assert!(set.contains(&"alice"));
        
        // And len counts unique elements
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_orset_convergence() {
        let mut s1 = ORSet::new();
        let mut s2 = ORSet::new();

        // Complex scenario
        s1.add("alice");
        s1.add("bob");
        
        s2.add("bob");
        s2.add("charlie");
        
        s1.remove(&"bob");  // s1 removes bob
        
        s2.add("bob");  // s2 adds bob again (concurrent)

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
        let mut s1 = ORSet::new();
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
        let mut set = LWWSet::new();
        
        set.add("alice");
        assert!(set.contains(&"alice"));
        
        set.remove("alice");
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn test_lwwset_last_write_wins() {
        let mut set = LWWSet::new();
        
        set.add("item");     // timestamp: 1
        set.remove("item");  // timestamp: 2 (later)
        
        // Remove wins (higher timestamp)
        assert!(!set.contains(&"item"));
        
        set.add("item");     // timestamp: 3 (even later)
        
        // Add wins now (highest timestamp)
        assert!(set.contains(&"item"));
    }

    #[test]
    fn test_lwwset_concurrent_different_timestamps() {
        let mut s1 = LWWSet::new();
        let mut s2 = LWWSet::new();

        s1.add("item");     // timestamp: 1
        s2.remove("item");  // timestamp: 1 (concurrent, same clock value)

        // In case of tie, we need bias rule
        // Our implementation: add wins on tie (t_add > t_rem is false, but t_add >= t_rem)
        // Let's test actual behavior:
        
        s1.merge(&s2);
        s2.merge(&s1);

        // With our implementation: max_added=1, max_removed=1
        // contains checks: t_add > t_rem → 1 > 1 = false
        assert!(!s1.contains(&"item"));  // Remove wins on tie
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_lwwset_convergence() {
        let mut s1 = LWWSet::new();
        let mut s2 = LWWSet::new();

        s1.add("alice");
        s1.add("bob");
        
        s2.add("charlie");
        s2.remove("bob");  // Higher timestamp than s1's add

        s1.merge(&s2);
        s2.merge(&s1);

        assert_eq!(s1, s2);
        assert!(s1.contains(&"alice"));
        assert!(s1.contains(&"charlie"));
        
        // bob's fate depends on timestamps
        // s2's remove has higher timestamp, so bob is removed
        assert!(!s1.contains(&"bob"));
    }

    #[test]
    fn test_lwwset_multiple_add_remove() {
        let mut set = LWWSet::new();
        
        set.add("item");     // t=1
        set.remove("item");  // t=2
        set.add("item");     // t=3
        set.remove("item");  // t=4
        set.add("item");     // t=5
        
        // Latest operation (add at t=5) wins
        assert!(set.contains(&"item"));
    }

    #[test]
    fn test_lwwset_idempotence() {
        let mut s1 = LWWSet::new();
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