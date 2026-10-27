//! Tamper-evident provenance chain: each mutation's hash covers the previous
//! head, so any reordering or removal breaks verification.
//!
//! Estate note: this is a minimal in-crate chain. The estate's `auditlog`
//! crate (same shape, queryable trail) is blocked on crates.io by a name
//! collision — once renamed, this module becomes an adapter to it.

use serde::Serialize;
use sha2::{Digest, Sha256};

const GENESIS: &str = "argus-ontology-genesis";

/// A single chained mutation record.
#[derive(Debug, Clone, Serialize)]
pub struct ChainEntry {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Previous head hash (hex).
    pub prev: String,
    /// This entry's hash (hex).
    pub hash: String,
    /// What happened (`link`, `entity:upsert`, ...).
    pub action: String,
    /// Payload digest input (human-readable summary).
    pub payload: String,
    /// When, epoch ms.
    pub at_ms: i64,
}

/// Append-only provenance chain state.
#[derive(Debug)]
pub struct ProvenanceChain {
    head: String,
    seq: u64,
    entries: Vec<ChainEntry>,
}

impl Default for ProvenanceChain {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvenanceChain {
    /// Chain starting from the genesis hash.
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: GENESIS.to_string(),
            seq: 0,
            entries: Vec::new(),
        }
    }

    /// Appends a record; returns the new head hash.
    pub fn append(&mut self, action: &str, payload: &str, at_ms: i64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.head.as_bytes());
        hasher.update(action.as_bytes());
        hasher.update(payload.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        let prev = std::mem::replace(&mut self.head, hash.clone());
        self.seq += 1;
        self.entries.push(ChainEntry {
            seq: self.seq,
            prev,
            hash,
            action: action.to_string(),
            payload: payload.to_string(),
            at_ms,
        });
        self.head.clone()
    }

    /// Current head hash.
    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Last `n` records, oldest first.
    #[must_use]
    pub fn tail(&self, n: usize) -> Vec<ChainEntry> {
        let skip = self.entries.len().saturating_sub(n);
        self.entries.iter().skip(skip).cloned().collect()
    }

    /// Re-verifies the whole chain; `true` when untampered.
    #[must_use]
    pub fn verify(&self) -> bool {
        let mut prev = GENESIS.to_string();
        for entry in &self.entries {
            let mut hasher = Sha256::new();
            hasher.update(prev.as_bytes());
            hasher.update(entry.action.as_bytes());
            hasher.update(entry.payload.as_bytes());
            let hash = format!("{:x}", hasher.finalize());
            if hash != entry.hash || entry.prev != prev {
                return false;
            }
            prev = hash;
        }
        prev == self.head
    }
}

#[cfg(test)]
mod tests {
    use super::ProvenanceChain;

    #[test]
    fn chain_verifies_after_appends() {
        let mut chain = ProvenanceChain::new();
        for i in 0..50 {
            let action = if i % 2 == 0 { "entity:upsert" } else { "link" };
            chain.append(action, &format!("payload-{i}"), 1_000 + i);
        }
        assert_eq!(chain.len(), 50);
        assert!(chain.verify(), "chain must verify after sequential appends");
    }

    #[test]
    fn default_chain_is_genesis_anchored() {
        // Regression: derive(Default) used to leave head empty, breaking
        // the genesis anchor of the first entry.
        let chain = ProvenanceChain::default();
        assert_eq!(chain.head(), super::GENESIS);
    }
}
