//! Set CRDTs
//!
//! This module implements various Set CRDTs with different semantics
//! for handling concurrent add/remove operations.

use std::{collections::HashSet, sync::mpsc::SendError};
use std::hash::Hash;
use crate::core::{ActorID, Crdt};

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