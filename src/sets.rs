//! Set CRDTs
//!
//! This module implements various Set CRDTs with different semantics
//! for handling concurrent add/remove operations.

use core::hash;
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

impl<T:Clone+Eq+Hash> Crdt for GSet<T> {
    fn merge(&mut self, other: &Self) {
        for element in &other.elements{
            self.elements.insert(element.clone());
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