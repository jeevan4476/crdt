//! CRDT library - Conflict-Free Replicated Data Types
//!
//! This library implements CRDTs for distributed systems.

pub mod clocks;
pub mod core;
pub mod counters;
pub mod maps;
pub mod sequences;
pub mod sets;

pub use clocks::VectorClock;
pub use core::{ActorID, Crdt};
pub use counters::GCounter;
pub use maps::{ORMap, ORMapDelta, ORMapValue};


