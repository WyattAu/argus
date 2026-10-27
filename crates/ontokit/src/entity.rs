//! Entity model.

use std::collections::BTreeMap;

use serde::Serialize;

/// What kind of real-world thing an entity represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// An event in the world (quake, fire, CVE, headline).
    Event,
    /// A geographic location.
    Place,
    /// A data source / publisher.
    Source,
    /// A tradable symbol.
    Symbol,
}

/// A typed entity with provenance.
#[derive(Debug, Clone, Serialize)]
pub struct Entity {
    /// Stable id (`event:usgs:abc123`, `place:60.2,-152.4`, ...).
    pub id: String,
    /// Entity type.
    pub kind: EntityKind,
    /// Human name.
    pub name: String,
    /// Extra attributes (magnitude, last price, ...).
    pub props: BTreeMap<String, String>,
    /// Feed that produced the entity.
    pub source: String,
    /// Data license, carried as data.
    pub license: String,
    /// First observation, epoch ms.
    pub first_seen_ms: i64,
    /// Most recent observation, epoch ms.
    pub last_seen_ms: i64,
}
