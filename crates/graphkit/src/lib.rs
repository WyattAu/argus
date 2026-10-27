//! graphkit — typed link graph for entity relationships.
//!
//! Nodes are string ids (owned by the caller, e.g. `ontokit` entities);
//! edges carry a list of [`Link`]s (one per predicate — parallel predicates
//! between the same pair merge into one edge). Queries: direct links in both
//! directions and bounded n-hop BFS with the path taken.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, VecDeque};

use petgraph::graph::DiGraph;
use petgraph::visit::EdgeRef;
use serde::Serialize;

/// One typed, time-stamped relationship between two nodes.
#[derive(Debug, Clone, Serialize)]
pub struct Link {
    /// Predicate slug (`published_by`, `occurred_at`, ...).
    pub predicate: String,
    /// When the relationship was first observed, epoch ms.
    pub since_ms: i64,
}

/// How a node was reached during an n-hop walk.
#[derive(Debug, Clone, Serialize)]
pub struct NeighborView {
    /// Neighbor node id.
    pub id: String,
    /// BFS depth (1 = direct neighbor).
    pub depth: usize,
    /// Node the neighbor was reached through.
    pub via: String,
    /// Predicate on the final hop.
    pub predicate: String,
}

/// Directed graph over string ids with typed links.
#[derive(Debug, Default)]
pub struct LinkGraph {
    graph: DiGraph<String, Vec<Link>>,
    index: HashMap<String, petgraph::graph::NodeIndex>,
}

impl LinkGraph {
    /// Empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn node_index(&mut self, id: &str) -> petgraph::graph::NodeIndex {
        if let Some(&idx) = self.index.get(id) {
            return idx;
        }
        let idx = self.graph.add_node(id.to_string());
        self.index.insert(id.to_string(), idx);
        idx
    }

    /// Adds (or merges) a directed link `from --predicate--> to`.
    /// Creates missing endpoints.
    pub fn add_link(&mut self, from: &str, to: &str, predicate: &str, since_ms: i64) {
        let a = self.node_index(from);
        let b = self.node_index(to);
        let link = Link {
            predicate: predicate.to_string(),
            since_ms,
        };
        match self.graph.find_edge(a, b).and_then(|edge| self.graph.edge_weight_mut(edge)) {
            Some(existing) => {
                if !existing.iter().any(|l| l.predicate == predicate) {
                    existing.push(link);
                }
            }
            None => {
                self.graph.add_edge(a, b, vec![link]);
            }
        }
    }

    /// Whether the node exists.
    #[must_use]
    pub fn has_node(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Node count.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Edge count (merged predicate groups, not individual links).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// All links touching `id`, both directions, as
    /// `(from, to, link)` triples.
    #[must_use]
    pub fn links_of(&self, id: &str) -> Vec<(String, String, Link)> {
        let Some(&idx) = self.index.get(id) else {
            return Vec::new();
        };
        let name = |node: petgraph::graph::NodeIndex| self.graph[node].clone();
        let mut out = Vec::new();
        for edge in self.graph.edges_directed(idx, petgraph::Direction::Outgoing) {
            for link in edge.weight() {
                out.push((id.to_string(), name(edge.target()), link.clone()));
            }
        }
        for edge in self.graph.edges_directed(idx, petgraph::Direction::Incoming) {
            for link in edge.weight() {
                out.push((name(edge.source()), id.to_string(), link.clone()));
            }
        }
        out
    }

    /// BFS within `hops` of `id`, following links in both directions.
    /// Returns each reached node with its depth, the node it was reached
    /// through, and the final-hop predicate. The origin is excluded.
    #[must_use]
    pub fn hop_neighbors(&self, id: &str, hops: usize) -> BTreeMap<String, NeighborView> {
        let mut views: BTreeMap<String, NeighborView> = BTreeMap::new();
        if hops == 0 || !self.has_node(id) {
            return views;
        }
        let mut depths: HashMap<String, usize> = HashMap::new();
        depths.insert(id.to_string(), 0);
        let mut queue = VecDeque::new();
        queue.push_back((id.to_string(), 0usize));
        while let Some((node, depth)) = queue.pop_front() {
            if depth >= hops {
                continue;
            }
            let next_depth = depth + 1;
            for (from, to, links) in self.links_of(&node) {
                let (neighbor, via) = if from == node {
                    (to, from.clone())
                } else {
                    (from, to.clone())
                };
                if neighbor == id {
                    continue;
                }
                let predicate = links.predicate;
                let view = NeighborView {
                    id: neighbor.clone(),
                    depth: next_depth,
                    via: via.clone(),
                    predicate,
                };
                let first = !depths.contains_key(&neighbor);
                if first {
                    depths.insert(neighbor.clone(), next_depth);
                    queue.push_back((neighbor.clone(), next_depth));
                    views.insert(neighbor.clone(), view);
                } else {
                    // Keep the shallowest path; same-depth re-discoveries
                    // refresh an empty predicate label only.
                    views.entry(neighbor.clone()).and_modify(|v| {
                        if next_depth < v.depth {
                            *v = view;
                        } else if next_depth == v.depth && v.predicate.is_empty() {
                            v.predicate.clone_from(&view.predicate);
                        }
                    });
                }
            }
        }
        views
    }
}
