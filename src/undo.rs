//! Collaborative Selective Undo/Redo mechanism for RGA sequences.
//!
//! Preserves user intent by maintaining a per-actor intention log.
//! Undo and redo operations target only the local actor's actions,
//! allowing concurrent edits from peers to remain unaffected.

use crate::core::ActorID;
use crate::sequences::{RGA, RGADelta, Timestamp};
use serde::{Deserialize, Serialize};

/// An operation stored in the actor's intention log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Operation<T: Clone + PartialEq> {
    Insert {
        timestamp: Timestamp,
        value: T,
        parent: Option<Timestamp>,
    },
    Remove {
        timestamp: Timestamp,
        target_timestamp: Timestamp,
        value: T,
    },
}

/// Per-actor intention log managing local undo and redo history.
#[derive(Clone, Debug)]
pub struct IntentionLog<T: Clone + PartialEq> {
    actor: ActorID,
    undo_stack: Vec<Operation<T>>,
    redo_stack: Vec<Operation<T>>,
}

impl<T: Clone + PartialEq> IntentionLog<T> {
    /// Create a new intention log for an actor.
    pub fn new(actor: ActorID) -> Self {
        IntentionLog {
            actor,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Return actor ID.
    pub fn actor(&self) -> ActorID {
        self.actor
    }

    /// Record a local insert operation into the intention log.
    pub fn record_insert(&mut self, timestamp: Timestamp, value: T, parent: Option<Timestamp>) {
        self.undo_stack.push(Operation::Insert {
            timestamp,
            value,
            parent,
        });
        self.redo_stack.clear();
    }

    /// Record a local remove operation into the intention log.
    pub fn record_remove(&mut self, timestamp: Timestamp, target_timestamp: Timestamp, value: T) {
        self.undo_stack.push(Operation::Remove {
            timestamp,
            target_timestamp,
            value,
        });
        self.redo_stack.clear();
    }

    /// Perform a selective undo for this actor on the target RGA sequence.
    pub fn undo(&mut self, rga: &mut RGA<T>) -> Option<RGADelta<T>> {
        let op = self.undo_stack.pop()?;
        match &op {
            Operation::Insert { timestamp, .. } => {
                let delta = RGADelta::Remove(timestamp.clone());
                rga.apply_delta(delta.clone());
                self.redo_stack.push(op);
                Some(delta)
            }
            Operation::Remove {
                target_timestamp, ..
            } => {
                rga.restore_vertex(target_timestamp);
                self.redo_stack.push(op);
                None
            }
        }
    }

    /// Perform a selective redo for this actor on the target RGA sequence.
    pub fn redo(&mut self, rga: &mut RGA<T>) -> Option<RGADelta<T>> {
        let op = self.redo_stack.pop()?;
        match &op {
            Operation::Insert { timestamp, .. } => {
                rga.restore_vertex(timestamp);
                self.undo_stack.push(op);
                None
            }
            Operation::Remove {
                target_timestamp, ..
            } => {
                let delta = RGADelta::Remove(target_timestamp.clone());
                rga.apply_delta(delta.clone());
                self.undo_stack.push(op);
                Some(delta)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Crdt;
    use crate::sequences::RGA;

    #[test]
    fn test_selective_undo_redo_local() {
        let mut rga = RGA::new(1);
        let mut log = IntentionLog::new(1);

        let d1 = rga.insert(0, 'a');
        if let RGADelta::Insert { vertex, .. } = &d1 {
            log.record_insert(vertex.timestamp.clone(), 'a', vertex.parent.clone());
        }

        let d2 = rga.insert(1, 'b');
        if let RGADelta::Insert { vertex, .. } = &d2 {
            log.record_insert(vertex.timestamp.clone(), 'b', vertex.parent.clone());
        }

        assert_eq!(rga.value(), vec!['a', 'b']);

        // Undo 'b'
        log.undo(&mut rga);
        assert_eq!(rga.value(), vec!['a']);

        // Redo 'b'
        log.redo(&mut rga);
        assert_eq!(rga.value(), vec!['a', 'b']);
    }

    #[test]
    fn test_selective_undo_with_concurrent_peer_edits() {
        let mut rga_a = RGA::new(1);
        let mut rga_b = RGA::new(2);

        let mut log_a = IntentionLog::new(1);

        // Actor A inserts 'A'
        let d_a1 = rga_a.insert(0, 'A');
        if let RGADelta::Insert { vertex, .. } = &d_a1 {
            log_a.record_insert(vertex.timestamp.clone(), 'A', vertex.parent.clone());
        }
        rga_b.merge(&rga_a);

        // Actor B inserts 'B' after 'A'
        let _d_b1 = rga_b.insert(1, 'B');
        rga_a.merge(&rga_b);

        // Actor A inserts 'C'
        let d_a2 = rga_a.insert(2, 'C');
        if let RGADelta::Insert { vertex, .. } = &d_a2 {
            log_a.record_insert(vertex.timestamp.clone(), 'C', vertex.parent.clone());
        }
        rga_b.merge(&rga_a);

        assert_eq!(rga_a.value(), vec!['A', 'B', 'C']);

        // Actor A undos their last action ('C')
        log_a.undo(&mut rga_a);
        assert_eq!(rga_a.value(), vec!['A', 'B']); // 'B' from peer is preserved!
    }
}
