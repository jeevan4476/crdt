//! Sequence CRDTs for collaborative editing
//!
//! Implements RGA (Replicated Growable Array) - a CRDT for maintaining
//! a mutable sequence with insert and delete operations.

use crate::core::{ActorID,Crdt};

/// Unique identifier for each element in the sequence
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp {
    pub clock: u64,
    pub actor: ActorID,
}

impl Timestamp {
    pub fn new(clock: u64, actor: ActorID)->Self{
        Timestamp { clock, actor }
    }
    pub fn beginning()->Self{
        Timestamp { clock: 0, actor: 0 }
    }
    pub fn end()->Self{
        Timestamp { clock: u64::MAX, actor: u64::MAX }
    }
}

/// A vertex in the RGA linked list
#[derive(Clone, Debug,PartialEq)]
struct Vertex<T: Clone> {
    value: T,
    timestamp: Timestamp,
    removed: bool,  // Tombstone flag
}

/// RGA: Replicated Growable Array
///
/// A CRDT sequence designed for collaborative text editing.
/// Supports concurrent insert and delete operations that converge.
///
/// **How it works:**
/// - Each element has a unique timestamp (Lamport clock + actor ID)
/// - Elements stored as linked list ordered by insertion time
/// - Concurrent inserts at same position ordered by timestamp
/// - Delete marks element as tombstone (doesn't remove physically)
///
/// **Use cases:**
/// - Collaborative text editors (Google Docs style)
/// - Shared todo lists
/// - Any mutable sequence with concurrent edits
#[derive(Clone, Debug )]
pub struct RGA<T: Clone+PartialEq> {
    actor: ActorID,
    clock: u64,
    vertices: Vec<Vertex<T>>,  // Linked list as vector
}

impl<T: Clone + PartialEq> PartialEq for RGA<T> {
    fn eq(&self, other: &Self) -> bool {
        // Two RGAs are equal if they have the same visible sequence
        self.value() == other.value()
    }
}
impl<T: Clone + PartialEq> Eq for RGA<T> {}

impl<T:Clone+PartialEq> RGA<T>{
    pub fn new(actor:ActorID)->Self{
        RGA { actor, clock: 0, vertices: Vec::new() }
    }

    pub fn insert(&mut self,position: usize,value:T){
        let timestamp = self.tick();

        let after_idx = if position==0{
            None
        }else{
            self.visible_position_to_index(position-1)
        };

        let new_vertex = Vertex{
            value,
            timestamp: timestamp.clone(),
            removed:false
        };

        let insert_idx = match  after_idx {
            None=>{
                self.vertices.iter().position(|v| v.timestamp<timestamp)
                .unwrap_or(self.vertices.len())
            }
            Some(after)=>{
                let mut idx = after+1;
                while idx < self.vertices.len() && self.vertices[idx].timestamp>timestamp{
                    idx+=1;
                }
                idx
            }
        };
        self.vertices.insert(insert_idx, new_vertex);
    }

    //mark the element as tombstone rather than physically removing it
    pub fn remove(&mut self,position: usize){
        if let Some(idx) = self.visible_position_to_index(position){
            self.vertices[idx].removed = true;
        } 
    }
    pub fn len(&self) -> usize {
        self.vertices.iter().filter(|v| !v.removed).count()
    }
    pub fn value(&self)->Vec<T>{
        self.vertices.iter().filter(|v| !v.removed).map(|v| v.value.clone()).collect()
    }

    fn tick(&mut self)->Timestamp{
        self.clock+=1;
        Timestamp::new(self.clock, self.actor)
    }
    fn visible_position_to_index(&self,position: usize)->Option<usize>{
        let mut visible_count = 0;
        for(idx,vertex) in self.vertices.iter().enumerate(){
            if !vertex.removed{
                if visible_count == position{
                    return Some(idx);
                }
                visible_count+=1;
            }
        }
        None
    }
}

impl RGA<char>{
    pub fn to_string(&self)->String{
        self.value().iter().collect()
    }
}

impl<T:Clone+PartialEq> Crdt for RGA<T>{
    fn merge(&mut self, other: &Self) {
        let mut merged = Vec::new();
        let mut i = 0; 
        let mut j =0;
        
        while i < self.vertices.len() && j<other.vertices.len(){
            if self.vertices[i].timestamp > other.vertices[j].timestamp{
                merged.push(self.vertices[i].clone());
                i+=1;
            }else if self.vertices[i].timestamp< other.vertices[j].timestamp{
                merged.push(other.vertices[j].clone());
                j+=1;
            }else{
                let mut vertex = self.vertices[i].clone();
                vertex.removed = vertex.removed|| other.vertices[j].removed;
                merged.push(vertex);
                i+=1;
                j+=1;
            }
        }
        while i < self.vertices.len(){
                merged.push(self.vertices[i].clone());
                i+=1;
            }
        while j<other.vertices.len(){
                merged.push(other.vertices[j].clone());
                j+=1;
            }
        self.vertices = merged.clone();
        self.clock = self.clock.max(other.clock);          
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rga_basic_insert() {
        let mut doc = RGA::new(1);
        
        doc.insert(0, 'H');
        doc.insert(1, 'i');
        
        assert_eq!(doc.to_string(), "Hi");
    }

    #[test]
    fn test_rga_insert_delete() {
        let mut doc = RGA::new(1);
        
        doc.insert(0, 'H');
        doc.insert(1, 'e');
        doc.insert(2, 'l');
        doc.insert(3, 'l');
        doc.insert(4, 'o');
        
        assert_eq!(doc.to_string(), "Hello");
        
        doc.remove(1);  // Remove 'e'
        assert_eq!(doc.to_string(), "Hllo");
    }

    #[test]
    fn test_rga_concurrent_insert_same_position() {
        let mut doc1 = RGA::new(1);
        let mut doc2 = RGA::new(2);

        // Both insert at position 0
        doc1.insert(0, 'A');
        doc2.insert(0, 'B');

        // Merge
        doc1.merge(&doc2);
        doc2.merge(&doc1);

        // Should converge (order determined by timestamp)
        assert_eq!(doc1.to_string(), doc2.to_string());
        
        // Actor 1's timestamp < Actor 2's (assuming same clock)
        // So 'A' comes before 'B'
        let result = doc1.to_string();
        assert!(result == "AB" || result == "BA");  // Depends on clock tie-breaking
    }

    #[test]
    fn test_rga_collaborative_editing() {
        let mut doc1 = RGA::new(1);
        let mut doc2 = RGA::new(2);

        // User 1 types "Hello"
        doc1.insert(0, 'H');
        doc1.insert(1, 'e');
        doc1.insert(2, 'l');
        doc1.insert(3, 'l');
        doc1.insert(4, 'o');

        // User 2 types "World"
        doc2.insert(0, 'W');
        doc2.insert(1, 'o');
        doc2.insert(2, 'r');
        doc2.insert(3, 'l');
        doc2.insert(4, 'd');

        // Merge
        doc1.merge(&doc2);
        doc2.merge(&doc1);

        // Should converge
        assert_eq!(doc1.to_string(), doc2.to_string());
        
        // Result should contain all characters
        let result = doc1.to_string();
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_rga_insert_middle_concurrent() {
        let mut doc1 = RGA::new(1);
        let mut doc2 = RGA::new(2);

        // Both start with "ac"
        doc1.insert(0, 'a');
        doc1.insert(1, 'c');
        doc2.merge(&doc1);

        // User 1 inserts 'b' in middle
        doc1.insert(1, 'b');

        // User 2 inserts 'X' in middle (concurrent)
        doc2.insert(1, 'X');

        // Merge
        doc1.merge(&doc2);
        doc2.merge(&doc1);

        // Should converge
        assert_eq!(doc1.to_string(), doc2.to_string());
        
        // Result: "a" + (b or X first) + (X or b second) + "c"
        let result = doc1.to_string();
        assert!(result == "abXc" || result == "aXbc");
    }

    #[test]
    fn test_rga_delete_concurrent() {
        let mut doc1 = RGA::new(1);
        let mut doc2 = RGA::new(2);

        // Both start with "abc"
        doc1.insert(0, 'a');
        doc1.insert(1, 'b');
        doc1.insert(2, 'c');
        doc2.merge(&doc1);

        // User 1 deletes 'b'
        doc1.remove(1);

        // User 2 also deletes 'b' (concurrent)
        doc2.remove(1);

        // Merge
        doc1.merge(&doc2);
        doc2.merge(&doc1);

        // Should converge to "ac"
        assert_eq!(doc1.to_string(), "ac");
        assert_eq!(doc2.to_string(), "ac");
    }

    #[test]
    fn test_rga_convergence() {
        let mut doc1 = RGA::new(1);
        let mut doc2 = RGA::new(2);
        let mut doc3 = RGA::new(3);

        // Three users editing concurrently
        doc1.insert(0, 'A');
        doc2.insert(0, 'B');
        doc3.insert(0, 'C');

        // Merge in different orders
        doc1.merge(&doc2);
        doc1.merge(&doc3);

        doc2.merge(&doc3);
        doc2.merge(&doc1);

        doc3.merge(&doc1);
        doc3.merge(&doc2);

        // All should converge
        assert_eq!(doc1, doc2);
        assert_eq!(doc2, doc3);
    }
    #[test]
fn test_rga_concurrent_typo_fix() {
    let mut doc1 = RGA::new(1);
    let mut doc2 = RGA::new(2);

    // Start with "teh cat"
    for c in "teh cat".chars() {
        doc1.insert(doc1.len(), c);
    }
    doc2.merge(&doc1);

    // User 1 fixes typo: "teh" → "the"
    doc1.remove(1);  // Remove 'e'
    doc1.remove(1);  // Remove 'h' (positions shift!)
    doc1.insert(1, 'h');
    doc1.insert(2, 'e');

    // User 2 adds " sat" at end (concurrent)
    doc2.insert(doc2.len(), ' ');
    doc2.insert(doc2.len(), 's');
    doc2.insert(doc2.len(), 'a');
    doc2.insert(doc2.len(), 't');

    // Merge
    doc1.merge(&doc2);
    doc2.merge(&doc1);

    println!("Result: {}", doc1.to_string());
    
    // Should contain both: fix + addition
    let result = doc1.to_string();
    assert!(result.contains("the"));
    assert!(result.contains("sat"));
}
}