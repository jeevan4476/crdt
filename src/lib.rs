//! CRDT library - Conflict-Free Replicated Data Types
//! 
//! This library implements CRDTs for distributed systems.

pub mod core;
pub mod counters;

pub use core::{ActorID, Crdt};
pub use counters::GCounter;