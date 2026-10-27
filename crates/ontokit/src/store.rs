//! The entity store: entities + links + provenance, with extractors from
//! platform events.

use std::collections::BTreeMap;

use feedkit::{Observation, Quote};
use serde::Serialize;

use crate::entity::{Entity, EntityKind};
use crate::provenance::ProvenanceChain;

/// Neighbor joined with its entity for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct NeighborEntity {
    /// Neighbor id.
    pub id: String,
    /// Entity type.
    pub kind: EntityKind,
    /// Human name.
    pub name: String,
    /// BFS depth.
    pub depth: usize,
    /// Node reached through.
    pub via: String,
    /// Final-hop predicate.
    pub predicate: String,
}

/// Extracted-entity result counts, for logging/health.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ExtractionStats {
    /// Entities created (vs updated).
    pub created: u64,
    /// Links created.
    pub links: u64,
}

/// Entities, links and provenance in one append-friendly store.
#[derive(Debug, Default)]
pub struct EntityStore {
    entities: BTreeMap<String, Entity>,
    graph: graphkit::LinkGraph,
    chain: ProvenanceChain,
}

impl EntityStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn upsert(
        &mut self,
        id: &str,
        kind: EntityKind,
        name: &str,
        source: &str,
        license: &str,
        at_ms: i64,
    ) -> bool {
        if let Some(entity) = self.entities.get_mut(id) {
            entity.last_seen_ms = entity.last_seen_ms.max(at_ms);
            return false;
        }
        self.entities.insert(
            id.to_string(),
            Entity {
                id: id.to_string(),
                kind,
                name: name.to_string(),
                props: BTreeMap::new(),
                source: source.to_string(),
                license: license.to_string(),
                first_seen_ms: at_ms,
                last_seen_ms: at_ms,
            },
        );
        self.chain.append("entity:upsert", id, at_ms);
        true
    }

    fn link(&mut self, from: &str, to: &str, predicate: &str, at_ms: i64) {
        self.graph.add_link(from, to, predicate, at_ms);
        self.chain
            .append("link", &format!("{from} -{predicate}-> {to}"), at_ms);
    }

    /// Ingests an observation: creates the event entity, its source, and (if
    /// geolocated) a place — plus the links between them.
    pub fn extract_observation(&mut self, o: &Observation) -> ExtractionStats {
        let mut stats = ExtractionStats::default();
        let event_id = format!("event:{}:{}", o.source, o.id);
        if self.upsert(
            &event_id,
            EntityKind::Event,
            &o.title,
            &o.source,
            &o.license,
            o.occurred_at_ms,
        ) {
            stats.created += 1;
        }

        let source_id = format!("source:{}", o.source);
        if self.upsert(
            &source_id,
            EntityKind::Source,
            &o.source,
            &o.source,
            &o.license,
            o.occurred_at_ms,
        ) {
            stats.created += 1;
        }
        self.link(&event_id, &source_id, "published_by", o.occurred_at_ms);
        stats.links += 1;

        if let Some((lat, lon)) = o.lat.zip(o.lon) {
            let place_id = format!("place:{lat:.2},{lon:.2}");
            let name = o.place.clone().unwrap_or_else(|| format!("{lat:.2}, {lon:.2}"));
            if self.upsert(
                &place_id,
                EntityKind::Place,
                &name,
                &o.source,
                &o.license,
                o.occurred_at_ms,
            ) {
                stats.created += 1;
            }
            self.link(&event_id, &place_id, "occurred_at", o.occurred_at_ms);
            stats.links += 1;
        }
        stats
    }

    /// Ingests a quote: upserts the symbol entity and its last price.
    pub fn observe_quote(&mut self, q: &Quote) -> ExtractionStats {
        let mut stats = ExtractionStats::default();
        let id = format!("symbol:{}", q.symbol.to_uppercase());
        if self.upsert(&id, EntityKind::Symbol, &q.symbol.to_uppercase(), "market", "venue data", q.ts_ms) {
            stats.created += 1;
        }
        if let Some(entity) = self.entities.get_mut(&id) {
            entity
                .props
                .insert("last_price".to_string(), format!("{:.4}", q.price));
        }
        stats
    }

    /// One entity by id.
    #[must_use]
    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Entities filtered by kind (or all), capped, newest-seen first.
    #[must_use]
    pub fn entities(&self, kind: Option<EntityKind>, limit: usize) -> Vec<Entity> {
        let mut all: Vec<Entity> = self
            .entities
            .values()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .cloned()
            .collect();
        all.sort_by_key(|e| std::cmp::Reverse(e.last_seen_ms));
        all.truncate(limit);
        all
    }

    /// Entity count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// n-hop neighbors of `id`, joined with entity data.
    #[must_use]
    pub fn neighbors(&self, id: &str, hops: usize) -> Vec<NeighborEntity> {
        self.graph
            .hop_neighbors(id, hops)
            .into_values()
            .filter_map(|view| {
                let entity = self.entities.get(&view.id)?;
                Some(NeighborEntity {
                    id: entity.id.clone(),
                    kind: entity.kind,
                    name: entity.name.clone(),
                    depth: view.depth,
                    via: view.via,
                    predicate: view.predicate,
                })
            })
            .collect()
    }

    /// Whether an entity with this id exists.
    #[must_use]
    pub fn has_entity(&self, id: &str) -> bool {
        self.entities.contains_key(id)
    }

    /// Provenance-chain head hash.
    #[must_use]
    pub fn provenance_head(&self) -> &str {
        self.chain.head()
    }

    /// Verifies the provenance chain end to end.
    #[must_use]
    pub fn verify_provenance(&self) -> bool {
        self.chain.verify()
    }

    /// Last `n` provenance records.
    #[must_use]
    pub fn provenance_tail(&self, n: usize) -> Vec<crate::provenance::ChainEntry> {
        self.chain.tail(n)
    }
}
