//! Counter CRDTs

use std::collections::HashMap;
use crate::core::{ActorID, Crdt};

/// G-Counter: Grow-only counter
#[derive(Clone, Debug)]
pub struct GCounter {
    actor: ActorID,
    counts: HashMap<ActorID, u64>,
}

impl GCounter {
    pub fn new(actor: ActorID) -> Self {
        GCounter { 
            actor, 
            counts: HashMap::new() 
        }
    }

    pub fn increment(&mut self) {
        *self.counts.entry(self.actor).or_insert(0) += 1;
    }

    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }
}

//custom PartialEq that ignores actor fields
impl PartialEq for GCounter{
    fn eq(&self, other: &Self) -> bool {
        self.counts == other.counts
    }
}

impl Eq for GCounter{}

impl Crdt for GCounter {
    fn merge(&mut self, other: &Self) {
        for (actor, &count) in &other.counts {
            let entry = self.counts.entry(*actor).or_insert(0);
            *entry = (*entry).max(count);  // Take max per actor
        }
    }
}

/// PN-Counter: Positive-Negative Counter
#[derive(Clone,Debug)]
pub struct PNCounter{
    actor: ActorID,
    increments: GCounter,
    decrements: GCounter
}

impl PartialEq for PNCounter {
    fn eq(&self, other: &Self) -> bool {
        self.increments == other.increments 
            && self.decrements == other.decrements
    }
}

impl Eq for PNCounter {}

impl PNCounter{
    pub fn new(actor: ActorID)->Self{
        PNCounter { actor, increments: GCounter::new(actor), decrements: GCounter::new(actor) }
    }
    pub fn increment(&mut self){
        self.increments.increment();
    }
    pub fn decrement(&mut self){
        self.decrements.increment();
    }
    pub fn value(&self)->i64{
        self.increments.value() as i64 - self.decrements.value() as i64
    }
}

impl Crdt for PNCounter{
    fn merge(&mut self, other: &Self) {
        self.increments.merge(&other.increments);
        self.decrements.merge(&other.decrements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convergence() {
        let mut c1 = GCounter::new(1);
        let mut c2 = GCounter::new(2);

        c1.increment();
        c1.increment();
        c2.increment();

        c1.merge(&c2);
        c2.merge(&c1);

        assert_eq!(c1.value(), 3);
        assert_eq!(c2.value(), 3);
        assert_eq!(c1, c2);  
    }

    #[test]
    fn test_commutativity() {
        let mut c1 = GCounter::new(1);
        c1.increment();

        let mut c2 = GCounter::new(2);
        c2.increment();

        let mut left = c1.clone();
        left.merge(&c2);

        let mut right = c2.clone();
        right.merge(&c1);

        assert_eq!(left, right);  
    }

    #[test]
    fn test_idempotence() {
        let mut c1 = GCounter::new(1);
        c1.increment();

        let mut c2 = GCounter::new(2);
        c2.increment();

        let mut once = c1.clone();
        once.merge(&c2);

        let mut twice = once.clone();
        twice.merge(&c2);

        assert_eq!(once, twice);
    }
}

#[cfg(test)]
mod pn_tests {
    use super::*;

    #[test]
    fn test_pncounter_increment_decrement() {
        let mut counter = PNCounter::new(1);
        
        counter.increment();
        counter.increment();
        assert_eq!(counter.value(), 2);
        
        counter.decrement();
        assert_eq!(counter.value(), 1);
    }

    #[test]
    fn test_pncounter_convergence() {
        let mut c1 = PNCounter::new(1);
        let mut c2 = PNCounter::new(2);

        // Actor 1: +2, -1 = 1
        c1.increment();
        c1.increment();
        c1.decrement();

        // Actor 2: +1, -2 = -1
        c2.increment();
        c2.decrement();
        c2.decrement();

        // Merge both ways
        c1.merge(&c2);
        c2.merge(&c1);

        // Both should converge to: (2+1) - (1+2) = 0
        assert_eq!(c1.value(), 0);
        assert_eq!(c2.value(), 0);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_pncounter_commutativity() {
        let mut c1 = PNCounter::new(1);
        c1.increment();
        c1.decrement();

        let mut c2 = PNCounter::new(2);
        c2.decrement();

        let mut left = c1.clone();
        left.merge(&c2);

        let mut right = c2.clone();
        right.merge(&c1);

        assert_eq!(left, right);
    }

    #[test]
    fn test_pncounter_negative_values() {
        let mut counter = PNCounter::new(1);
        
        counter.decrement();
        counter.decrement();
        assert_eq!(counter.value(), -2);
        
        counter.increment();
        assert_eq!(counter.value(), -1);
    }
}