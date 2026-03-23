# CRDT

An ongoing Rust project for building a **Conflict-Free Replicated Data Types** (CRDT) library from scratch. The goal is to understand distributed convergence deeply, implement multiple CRDT families, and evolve the library from core data structures into synchronization, performance, and real application demos.

This is not a finished product. It is an actively developing systems project with a clear roadmap: core CRDTs first, then maps and sync protocols, then performance work, advanced CRDTs, demos, and production features.

## Project Status

Current status:

- Counters: completed
- Sets: completed
- Sequences: basic RGA completed
- Maps: current priority
- Sync protocols, serialization, persistence, demos: planned


## Why This Project Exists

In distributed systems, replicas often receive updates out of order, receive the same update more than once, or process concurrent operations from different nodes. CRDTs solve this by defining merge rules that are:

- Commutative
- Associative
- Idempotent

If those rules hold, replicas can merge state in any order and still end up with the same result.

That is the central idea this repository demonstrates.

## What Is Implemented So Far

### Core

- `src/core.rs`: defines the shared `Crdt` trait and the `ActorID` type.

```rust
pub trait Crdt: Clone + PartialEq {
    fn merge(&mut self, other: &Self);
}
```

Every CRDT in the project implements `merge()` as the convergence mechanism.

### Counters

- `GCounter`: grow-only counter
- `PNCounter`: increment/decrement counter built from two `GCounter`s

Implementation notes:

- `GCounter` stores a per-actor map of counts.
- Merge takes the maximum count per actor.
- Total value is the sum across actors.
- `PNCounter` models negative values by tracking increments and decrements separately.

Good interview angle:
This is a clean example of state-based CRDT design because it shows how a monotonic structure can still model a richer data type.

### Sets

- `GSet`: add-only set
- `TwoPSet`: add/remove set where removed elements cannot be added again
- `ORSet`: observed-remove set with unique tags, where concurrent add can survive remove
- `LWWSet`: last-write-wins set using timestamps

Why these are interesting:

- They show that "set with remove" is not one thing.
- Conflict resolution semantics are a design choice, not just an implementation detail.
- Each variant encodes a different product decision:
  - `TwoPSet`: removal is irreversible
  - `ORSet`: add-wins under concurrency
  - `LWWSet`: latest timestamp wins

### Sequences

- `RGA` (Replicated Growable Array): a sequence CRDT for collaborative editing

Implementation notes:

- Each inserted element gets a logical timestamp: `(clock, actor)`.
- Deletes are tombstones instead of physical removal.
- Merge combines vertices from both replicas and preserves tombstone state.
- The visible document is derived by filtering out removed entries.

This is the most interview-worthy structure in the repo because it moves beyond counters and sets into collaborative editing, which is where CRDTs become more concrete and memorable.

## Project Structure

```text
src/
  core.rs        # shared CRDT trait
  counters.rs    # GCounter, PNCounter
  sets.rs        # GSet, TwoPSet, ORSet, LWWSet
  sequences.rs   # RGA sequence CRDT
  lib.rs         # module exports
```

## Running The Project

This is a library crate, so the main way to exercise it is through tests:

```bash
cargo test
```

Useful local checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## CI Pipeline

This project now includes a GitHub Actions workflow at `.github/workflows/ci.yml`.

The repository also pins the Rust compiler with `rust-toolchain.toml` so local development and CI use the same toolchain version.

It runs on every `push` and `pull_request` and performs three checks:

- `cargo fmt --check`: verifies consistent formatting
- `cargo clippy --all-targets --all-features -- -D warnings`: treats lint warnings as CI failures
- `cargo test --all-targets --all-features`: runs the full test suite

Why this is useful for this project:

- CRDT code is correctness-heavy, so regressions should be caught early.
- merge semantics are easy to break accidentally, so tests should run automatically on every change.
- formatting and linting keep the codebase readable as the project grows into maps, sync, and more advanced CRDTs.

## Basic CD

This project now also includes a basic release workflow at `.github/workflows/release.yml`.

It triggers on tags that match `v*`, for example `v0.1.0`, and it:

- verifies the tagged commit is reachable from `main`
- runs the test suite again
- creates a GitHub release with generated release notes

This is a simple form of CD because it automates a delivery step after code has already passed CI.

## GitHub Setup

Recommended branch protection for `main`:

- require a pull request before merging
- require passing status checks before merging
- require branches to be up to date before merging

Recommended required checks:

- `Format Check`
- `Clippy`
- `Test Suite`

The local branch in this repository is currently `master`. If you want the release workflow and branch protection setup to work exactly as documented, rename the default branch to `main` on GitHub or adjust the workflow to use `master`.

More contributor and release guidance is in `CONTRIBUTING.md`.

## Example Usage

### Grow-only Counter

```rust
use crdt::{Crdt, GCounter};

let mut a = GCounter::new(1);
let mut b = GCounter::new(2);

a.increment();
a.increment();
b.increment();

a.merge(&b);
b.merge(&a);

assert_eq!(a.value(), 3);
assert_eq!(b.value(), 3);
```

### Observed-Remove Set

```rust
use crdt::core::Crdt;
use crdt::sets::ORSet;

let mut a = ORSet::new();
let mut b = ORSet::new();

a.add("alice");
b.add("alice");
a.remove(&"alice");

a.merge(&b);
b.merge(&a);

assert!(a.contains(&"alice"));
assert!(b.contains(&"alice"));
```

## What This Project Demonstrates

### 1. State-based CRDT design

Each data structure stores enough state so a replica can merge another replica without replaying an operation log.

### 2. Conflict resolution as semantics

The project shows that concurrent conflict resolution depends on the data model:

- Max-per-actor for counters
- Union for grow-only sets
- Tombstones for irreversible removal
- Tags for observed-remove behavior
- Timestamps for last-write-wins

### 3. Convergence testing

The test suite checks the practical behavior you care about in CRDTs:

- replicas converge after merge
- repeated merges do not corrupt state
- merge order should not matter

## Current Limitations

This part matters in interviews because it shows engineering judgment and makes it clear the project is still evolving.

- The library is in-memory only. There is no networking, serialization, storage layer, delta sync, or anti-entropy protocol yet.
- `ORSet` uses a local counter for tags. In a production system, tags should be globally unique across replicas.
- `LWWSet` uses a simple logical clock but does not model full causality or clock skew concerns.
- `RGA` is a simplified sequence CRDT and does not implement the full metadata and ordering machinery you would want for a production text editor.
- Map CRDTs are not implemented yet.
- There is no benchmarking, property-based testing, fuzz testing, docs deployment, or crate publishing yet.

## Roadmap

The roadmap is intentionally staged so each phase builds on the previous one.

### Phase 1: Core library

Completed:

- counters: `GCounter`, `PNCounter`
- sets: `GSet`, `TwoPSet`, `ORSet`, `LWWSet`
- sequences: basic `RGA`

Current priority:

- `ORMap`
- `LWWMap`
- tests for concurrent updates and merges

Next in core library:

- delta-state optimization
- Merkle tree based sync
- anti-entropy protocols
- examples, benchmarks, API docs, tutorial

### Phase 2: Advanced features

- RGA tombstone garbage collection
- RGA performance optimization
- range operations such as `insert_str` and `remove_range`
- undo/redo support
- cursor and selection tracking
- advanced sets such as `PNSet` and `USet`
- graph CRDTs
- alternative sequence CRDTs such as Logoot and Treedoc

### Phase 3+: Research, applications, and production work

- nested JSON CRDT composition
- vector clocks and dotted version vectors
- delta-state and Merkle sync strategies
- collaborative editor, shopping cart, todo list, whiteboard demos
- serialization, persistence, transport layer
- property-based tests, fuzzing, benchmarking, CI/CD
- advanced topics such as SEC formalization, partial replication, and hybrid CRDT/consensus models

## Reference

- [Conflict-free Replicated Data Types paper (INRIA)](https://inria.hal.science/inria-00555588/document)
