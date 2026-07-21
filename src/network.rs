//! Network transport layer with `Node` abstraction and Gossip Protocol.
//!
//! Enables peer-to-peer delta dissemination and anti-entropy full-state reconciliation.

use crate::clocks::VectorClock;
use crate::core::{ActorID, ApplyDelta, Crdt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Protocol message types exchanged between nodes in a gossip cluster.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GossipMessage<C, D> {
    /// Incremental delta update broadcast by an actor.
    DeltaBroadcast {
        origin: ActorID,
        clock: VectorClock,
        delta: D,
    },
    /// Request for full anti-entropy reconciliation containing sender's vector clock.
    AntiEntropyRequest { sender: ActorID, clock: VectorClock },
    /// Response to anti-entropy request containing responder's full CRDT state and vector clock.
    AntiEntropyResponse {
        sender: ActorID,
        clock: VectorClock,
        state: C,
    },
}

/// A node participating in a peer-to-peer CRDT network cluster.
#[derive(Clone, Debug)]
pub struct Node<C, D> {
    actor: ActorID,
    state: C,
    clock: VectorClock,
    peers: HashSet<ActorID>,
    _phantom: std::marker::PhantomData<D>,
}

impl<C, D> Node<C, D>
where
    C: Crdt + ApplyDelta<D> + Clone,
    D: Clone,
{
    /// Create a new node with a given ActorID and initial state.
    pub fn new(actor: ActorID, initial_state: C) -> Self {
        let mut clock = VectorClock::new();
        clock.tick(actor);
        Node {
            actor,
            state: initial_state,
            clock,
            peers: HashSet::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Return node's ActorID.
    pub fn actor(&self) -> ActorID {
        self.actor
    }

    /// Return reference to current CRDT state.
    pub fn state(&self) -> &C {
        &self.state
    }

    /// Return mutable reference to CRDT state.
    pub fn state_mut(&mut self) -> &mut C {
        &mut self.state
    }

    /// Return reference to node's vector clock.
    pub fn clock(&self) -> &VectorClock {
        &self.clock
    }

    /// Register a peer ActorID.
    pub fn add_peer(&mut self, peer: ActorID) {
        if peer != self.actor {
            self.peers.insert(peer);
        }
    }

    /// List connected peers.
    pub fn peers(&self) -> &HashSet<ActorID> {
        &self.peers
    }

    /// Apply a locally generated delta and prepare a broadcast message for peers.
    pub fn apply_local_delta(&mut self, delta: D) -> GossipMessage<C, D> {
        self.clock.tick(self.actor);
        self.state.apply_delta(delta.clone());
        GossipMessage::DeltaBroadcast {
            origin: self.actor,
            clock: self.clock.clone(),
            delta,
        }
    }

    /// Initiate anti-entropy state reconciliation request to peers.
    pub fn initiate_anti_entropy(&self) -> GossipMessage<C, D> {
        GossipMessage::AntiEntropyRequest {
            sender: self.actor,
            clock: self.clock.clone(),
        }
    }

    /// Process an incoming gossip message from another node.
    /// May return a response message if required (e.g., answering anti-entropy).
    pub fn handle_message(&mut self, msg: GossipMessage<C, D>) -> Option<GossipMessage<C, D>> {
        match msg {
            GossipMessage::DeltaBroadcast {
                origin,
                clock,
                delta,
            } => {
                self.clock.merge(&clock);
                self.clock.witness(origin, clock.get(origin));
                self.state.apply_delta(delta);
                None
            }
            GossipMessage::AntiEntropyRequest { sender, clock } => {
                self.add_peer(sender);
                self.clock.merge(&clock);
                Some(GossipMessage::AntiEntropyResponse {
                    sender: self.actor,
                    clock: self.clock.clone(),
                    state: self.state.clone(),
                })
            }
            GossipMessage::AntiEntropyResponse {
                sender,
                clock,
                state,
            } => {
                self.add_peer(sender);
                self.clock.merge(&clock);
                self.state.merge(&state);
                None
            }
        }
    }
}

/// In-memory network mesh simulating asynchronous gossip message routing.
#[derive(Debug)]
pub struct NetworkMesh<C, D> {
    nodes: HashMap<ActorID, Node<C, D>>,
}

impl<C, D> Default for NetworkMesh<C, D>
where
    C: Crdt + ApplyDelta<D> + Clone,
    D: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C, D> NetworkMesh<C, D>
where
    C: Crdt + ApplyDelta<D> + Clone,
    D: Clone,
{
    /// Create an empty network mesh.
    pub fn new() -> Self {
        NetworkMesh {
            nodes: HashMap::new(),
        }
    }

    /// Add a node to the mesh.
    pub fn add_node(&mut self, node: Node<C, D>) {
        self.nodes.insert(node.actor(), node);
    }

    /// Get reference to a node.
    pub fn get_node(&self, actor: ActorID) -> Option<&Node<C, D>> {
        self.nodes.get(&actor)
    }

    /// Get mutable reference to a node.
    pub fn get_node_mut(&mut self, actor: ActorID) -> Option<&mut Node<C, D>> {
        self.nodes.get_mut(&actor)
    }

    /// Connect all nodes as peers to each other.
    pub fn fully_connect(&mut self) {
        let actors: Vec<ActorID> = self.nodes.keys().copied().collect();
        for &a in &actors {
            for &b in &actors {
                if a == b {
                    continue;
                }
                if let Some(node) = self.nodes.get_mut(&a) {
                    node.add_peer(b);
                }
            }
        }
    }

    /// Broadcast a delta update from `sender` to all connected peer nodes.
    pub fn broadcast_delta(&mut self, sender: ActorID, delta: D) {
        if let Some(node) = self.nodes.get_mut(&sender) {
            let msg = node.apply_local_delta(delta);
            let peers: Vec<ActorID> = node.peers().iter().copied().collect();
            for peer in peers {
                if let Some(peer_node) = self.nodes.get_mut(&peer) {
                    peer_node.handle_message(msg.clone());
                }
            }
        }
    }

    /// Trigger anti-entropy sync between all nodes until full convergence.
    pub fn sync_all(&mut self) {
        let actors: Vec<ActorID> = self.nodes.keys().copied().collect();
        for &sender in &actors {
            let req = if let Some(node) = self.nodes.get(&sender) {
                node.initiate_anti_entropy()
            } else {
                continue;
            };

            for &receiver in &actors {
                if sender == receiver {
                    continue;
                }
                let resp = match self.nodes.get_mut(&receiver) {
                    Some(receiver_node) => receiver_node.handle_message(req.clone()),
                    None => None,
                };
                if let (Some(resp_msg), Some(sender_node)) = (resp, self.nodes.get_mut(&sender)) {
                    sender_node.handle_message(resp_msg);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sets::ORSet;

    #[test]
    fn test_gossip_mesh_delta_dissemination() {
        let mut mesh = NetworkMesh::<ORSet<String>, _>::new();
        mesh.add_node(Node::new(1, ORSet::new(1)));
        mesh.add_node(Node::new(2, ORSet::new(2)));
        mesh.add_node(Node::new(3, ORSet::new(3)));
        mesh.fully_connect();

        let delta = mesh
            .get_node_mut(1)
            .unwrap()
            .state_mut()
            .add("hello".to_string());
        mesh.broadcast_delta(1, delta);

        assert!(
            mesh.get_node(1)
                .unwrap()
                .state()
                .contains(&"hello".to_string())
        );
        assert!(
            mesh.get_node(2)
                .unwrap()
                .state()
                .contains(&"hello".to_string())
        );
        assert!(
            mesh.get_node(3)
                .unwrap()
                .state()
                .contains(&"hello".to_string())
        );
    }

    #[test]
    fn test_anti_entropy_convergence() {
        let mut mesh = NetworkMesh::<ORSet<String>, _>::new();
        mesh.add_node(Node::new(1, ORSet::new(1)));
        mesh.add_node(Node::new(2, ORSet::new(2)));
        mesh.fully_connect();

        // Node 1 & 2 modify state independently without broadcasting
        mesh.get_node_mut(1)
            .unwrap()
            .state_mut()
            .add("node1_data".to_string());
        mesh.get_node_mut(2)
            .unwrap()
            .state_mut()
            .add("node2_data".to_string());

        // Perform anti-entropy sync
        mesh.sync_all();

        assert_eq!(
            mesh.get_node(1).unwrap().state(),
            mesh.get_node(2).unwrap().state()
        );
        assert!(
            mesh.get_node(1)
                .unwrap()
                .state()
                .contains(&"node1_data".to_string())
        );
        assert!(
            mesh.get_node(1)
                .unwrap()
                .state()
                .contains(&"node2_data".to_string())
        );
    }
}
