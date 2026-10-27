//! ontokit — entity ontology with provenance for the argus platform.
//!
//! The [`EntityStore`] ingests platform events ([`feedkit::Observation`],
//! [`feedkit::Quote`]) and maintains typed entities, a [`graphkit::LinkGraph`]
//! of relationships, and a tamper-evident SHA-256 provenance chain over every
//! mutation. Entities extracted from one feed link naturally to entities from
//! another — the same place shows up in quake and wildfire events.

#![forbid(unsafe_code)]

mod entity;
mod extract;
mod provenance;
mod store;

pub use entity::{Entity, EntityKind};
pub use provenance::ProvenanceChain;
pub use store::{EntityStore, NeighborEntity};
