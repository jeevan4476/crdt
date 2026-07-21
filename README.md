# crdt

A production-grade Rust library implementing **Conflict-Free Replicated Data Types** — the foundational primitives for building distributed systems where concurrent writes always converge without coordination.

[![CI](https://github.com/jeevan4476/crdt/actions/workflows/ci.yml/badge.svg)](https://github.com/jeevan4476/crdt/actions/workflows/ci.yml)

---

## What's Implemented

| Module | Types | Notes |
|---|---|---|
| `counters` | `GCounter`, `PNCounter` | Per-actor maps, merge via pointwise max |
| `sets` | `GSet`, `TwoPSet`, `ORSet`, `LWWSet` | Four distinct conflict semantics |
| `sequences` | `RGA` | Topological parent-aware merge, tombstone GC |
| `maps` | `ORMap` | Nested heterogeneous CRDT document, Update-Wins |
| `clocks` | `VectorClock` | Causal ordering, concurrency detection, auto-prune watermark |
| `storage` | `Wal`, `WalRecord` | Durable crash recovery with Write-Ahead Log & Snapshots |
| `network` | `Node`, `NetworkMesh`, `GossipMessage` | P2P node abstraction & gossip anti-entropy reconciliation |
| `undo` | `IntentionLog`, `Operation` | Per-actor intention-preserving selective undo/redo |
| `richtext` | `RichText`, `SpanMark`, `FormatValue` | Peritext-style format spans anchored to character timestamps |

All types implement `serde` (`Serialize` / `Deserialize`) and `wincode` (`SchemaWrite` / `SchemaRead`) for binary network transport. Every mutable operation returns a typed **Delta** enabling operation-based sync without shipping full state.

---

## Quick Start

```rust
use crdt::{Crdt, GCounter};

let mut a = GCounter::new(1);
let mut b = GCounter::new(2);
a.increment();
b.increment();
a.merge(&b);
assert_eq!(a.value(), 2); // always converges
```

```rust
use crdt::sets::ORSet;
use crdt::Crdt;

// Concurrent add beats concurrent remove (Add-Wins)
let mut a = ORSet::new(1);
let mut b = ORSet::new(2);
a.add("alice");
b.add("alice");
a.remove(&"alice"); // a removes — b's add was concurrent

a.merge(&b);
assert!(a.contains(&"alice")); // alice survives
```

```rust
use crdt::{ORMap, ORMapValue};
use crdt::counters::PNCounter;
use crdt::Crdt;

// Nested document: two replicas edit different fields
let mut doc_a: ORMap<String> = ORMap::new(1);
let mut doc_b: ORMap<String> = ORMap::new(2);

let mut score = PNCounter::new(1);
score.increment();
doc_a.insert("score".to_string(), ORMapValue::Counter(score));

doc_b.merge(&doc_a); // converges
assert!(doc_b.contains_key(&"score".to_string()));
```

```rust
use crdt::{Node, NetworkMesh, ORSet};

// P2P Gossip Dissemination
let mut mesh = NetworkMesh::<ORSet<String>, _>::new();
mesh.add_node(Node::new(1, ORSet::new(1)));
mesh.add_node(Node::new(2, ORSet::new(2)));
mesh.fully_connect();

let delta = mesh.get_node_mut(1).unwrap().state_mut().add("data".to_string());
mesh.broadcast_delta(1, delta);
assert!(mesh.get_node(2).unwrap().state().contains(&"data".to_string()));
```

---

## Architecture

### Conflict Semantics

Each type encodes a deliberate conflict resolution policy — not a default:

| Type | Concurrent add + remove | Tie resolution |
|---|---|---|
| `TwoPSet` | Remove wins, permanently | N/A |
| `ORSet` | Add wins (unique per-actor tags) | Any concurrent add survives |
| `LWWSet` | Latest timestamp wins | `(clock, ActorID)` tuple, deterministic |
| `ORMap` | Update wins over remove | Recursive inner-CRDT merge |

### Delta Protocol

All writes emit typed algebraic deltas, enabling $O(1)$ network payloads instead of full state shipping:

```rust
let delta: ORSetDelta<&str> = set.add("alice");   // → ORSetDelta::Add("alice", tag)
let delta: RGADelta<char>   = rga.insert(0, 'H'); // → RGADelta::Insert { vertex, .. }

// Remote node applies just the delta — no full sync needed
other_node.apply_delta(delta);
```

### Causal Coordination

`VectorClock` tracks per-actor logical time across the cluster and computes the global safe watermark for tombstone GC:

```rust
use crdt::VectorClock;

let mut node_a = VectorClock::new();
let mut node_b = VectorClock::new();
node_a.tick(1);
node_b.tick(2);

assert!(node_a.concurrent(&node_b)); // independent edits detected

// Compute global prune floor — any tombstone ≤ watermark is safe to delete
let watermark = node_a.watermark(&[node_b]);
rga.prune(watermark); // automatic GC, no manual guessing
```

---

## Running

```bash
cargo test                                          # full suite
cargo clippy --all-targets -- -D warnings           # zero lint policy
cargo fmt --check                                   # formatting gate
```

---

## CI / CD

Every push and pull request runs:

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test --all-targets`

Releases are automated via `.github/workflows/release.yml` on semver tags (`v*`).  
The compiler version is pinned via `rust-toolchain.toml` — CI and local builds are always identical.

---

## Roadmap

- [x] `GCounter`, `PNCounter`
- [x] `GSet`, `TwoPSet`, `ORSet`, `LWWSet`
- [x] `RGA` with topological merge and tombstone GC
- [x] Delta events for all mutable operations
- [x] `serde` + `wincode` serialization
- [x] `ORMap` — nested heterogeneous document CRDT
- [x] `VectorClock` — causal ordering + auto-prune watermark
- [x] Persistent storage — WAL + snapshot (crash recovery)
- [x] Network transport — `Node` abstraction + gossip protocol
- [x] Collaborative undo/redo — per-actor intention log on RGA
- [x] Rich text formatting — Peritext-style format spans on RGA

---

## References

- [Shapiro et al. — CRDTs: A comprehensive study (INRIA, 2011)](https://inria.hal.science/inria-00555588/document)
- [RGA — Replicated Growable Array (Roh et al., 2011)](https://doi.org/10.1016/j.jpdc.2010.12.006)
- [Peritext — A CRDT for Rich-Text Collaboration (Litt et al., 2022)](https://www.inkandswitch.com/peritext/)
