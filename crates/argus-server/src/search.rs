//! Full-text search over observations (tantivy, in-RAM).
//!
//! Estate note: this is raw tantivy behind a tiny facade. The estate's
//! `tantivy-ext` (autocomplete/highlight wrapper) is blocked on crates.io by
//! a name collision — once renamed, this module becomes an adapter to it.

use std::sync::Mutex;

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{OwnedValue, Schema};
use tantivy::{DocAddress, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

/// A ranked search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// Observation id (`event:<source>:<id>` in the ontology).
    pub id: String,
    /// Headline.
    pub title: String,
    /// Observation kind.
    pub kind: String,
    /// Source feed.
    pub source: String,
    /// Relevance score.
    pub score: f32,
}

/// In-memory search index.
pub struct SearchIndex {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    doc_id: tantivy::schema::Field,
    title: tantivy::schema::Field,
    kind: tantivy::schema::Field,
    source: tantivy::schema::Field,
}

impl SearchIndex {
    /// Creates an empty in-RAM index.
    ///
    /// # Errors
    /// Returns the tantivy error on index/writer construction failure.
    pub fn new() -> tantivy::Result<Self> {
        let mut builder = Schema::builder();
        let doc_id =
            builder.add_text_field("doc_id", tantivy::schema::STRING | tantivy::schema::STORED);
        let title =
            builder.add_text_field("title", tantivy::schema::TEXT | tantivy::schema::STORED);
        let kind =
            builder.add_text_field("kind", tantivy::schema::STRING | tantivy::schema::STORED);
        let source =
            builder.add_text_field("source", tantivy::schema::STRING | tantivy::schema::STORED);
        let schema = builder.build();
        let index = Index::create_in_ram(schema);
        let writer = index.writer(50_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            doc_id,
            title,
            kind,
            source,
        })
    }

    /// Adds (and commits) one document.
    ///
    /// # Errors
    /// Returns the tantivy error on write/commit failure.
    pub fn add(&self, id: &str, title: &str, kind: &str, source: &str) -> tantivy::Result<()> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.doc_id, id);
        doc.add_text(self.title, title);
        doc.add_text(self.kind, kind);
        doc.add_text(self.source, source);
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        writer.add_document(doc)?;
        writer.commit()?;
        Ok(())
    }

    /// Runs a query over title/kind/source, returning up to `limit` hits.
    ///
    /// # Errors
    /// Returns the tantivy error on parse/search failure.
    pub fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<SearchHit>> {
        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.title, self.kind, self.source]);
        let parsed = parser.parse_query(query)?;
        let collector = TopDocs::with_limit(limit).order_by_score();
        let top_docs = searcher.search(parsed.as_ref(), &collector)?;
        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, address) in top_docs {
            let doc: TantivyDocument = searcher.doc(address)?;
            hits.push(SearchHit {
                id: get_text(&doc, self.doc_id, address),
                title: get_text(&doc, self.title, address),
                kind: get_text(&doc, self.kind, address),
                source: get_text(&doc, self.source, address),
                score,
            });
        }
        Ok(hits)
    }
}

/// Extracts the first stored text value, falling back to the doc address.
fn get_text(doc: &TantivyDocument, field: tantivy::schema::Field, address: DocAddress) -> String {
    doc.get_first(field)
        .map(OwnedValue::from)
        .and_then(|owned| match owned {
            OwnedValue::Str(text) => Some(text),
            _ => None,
        })
        .unwrap_or_else(|| format!("doc@{}", address.doc_id))
}
