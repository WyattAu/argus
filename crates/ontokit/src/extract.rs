//! Extraction helpers bridging platform events into the ontology.
//!
//! Kept separate from `store.rs` so the ingest surface can grow (NLP-derived
//! entities, user annotations) without bloating the store type.

use feedkit::{Observation, Quote};

use crate::store::{EntityStore, ExtractionStats};

impl EntityStore {
    /// Convenience wrapper matching the bus bridge call shape.
    pub fn ingest_observation(&mut self, observation: &Observation) -> ExtractionStats {
        self.extract_observation(observation)
    }

    /// Convenience wrapper matching the bus bridge call shape.
    pub fn ingest_quote(&mut self, quote: &Quote) -> ExtractionStats {
        self.observe_quote(quote)
    }
}
