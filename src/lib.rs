//! CRDT library - Conflict-Free Replicated Data Types
//!
//! Production-grade Rust library implementing Conflict-Free Replicated Data Types,
//! persistent Write-Ahead Logging (WAL), gossip network transport, collaborative undo/redo,
//! and Peritext rich-text formatting.

pub mod clocks;
pub mod core;
pub mod counters;
pub mod maps;
pub mod network;
pub mod richtext;
pub mod sequences;
pub mod sets;
pub mod storage;
pub mod undo;

pub use clocks::VectorClock;
pub use core::{ActorID, ApplyDelta, Crdt};
pub use counters::{GCounter, PNCounter};
pub use maps::{ORMap, ORMapDelta, ORMapValue};
pub use network::{GossipMessage, NetworkMesh, Node};
pub use richtext::{FormatValue, PeritextDelta, RichText, SpanMark};
pub use sequences::{RGA, RGADelta, Timestamp, Vertex};
pub use sets::{GSet, LWWSet, ORSet, TwoPSet};
pub use storage::{Wal, WalRecord};
pub use undo::{IntentionLog, Operation};
