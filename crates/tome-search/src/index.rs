//! Index abstraction. Owns the Tantivy `Index` and exposes both a streaming
//! [`Writer`] for building/appending and a [`search`](Index::search) method
//! for querying.

use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{Index as TantivyIndex, IndexWriter, TantivyDocument, Term, doc};
use tome_core::{Result, SearchHit, Searcher, Tier, TomeError};

use crate::schema::TomeSchema;

/// Suggested write buffer per Tantivy guidance: 50–200 MB. Smaller buffers
/// mean more frequent commits but lower peak memory.
pub const DEFAULT_WRITER_BUFFER_BYTES: usize = 50 * 1024 * 1024;

pub struct Index {
    inner: TantivyIndex,
    schema: TomeSchema,
}

impl Index {
    /// Create a fresh in-memory index. Useful for tests; production indexes
    /// always live on disk.
    pub fn create_in_ram() -> Result<Self> {
        let schema = TomeSchema::build();
        let inner = TantivyIndex::create_in_ram(schema.schema.clone());
        Ok(Self { inner, schema })
    }

    /// Create a fresh on-disk index at `path`. The directory must exist and
    /// be empty (or contain a non-Tantivy structure that we can wipe).
    pub fn create_in_dir(path: &Path) -> Result<Self> {
        let schema = TomeSchema::build();
        let inner = TantivyIndex::create_in_dir(path, schema.schema.clone())
            .map_err(|e| TomeError::Other(format!("create index: {e}")))?;
        Ok(Self { inner, schema })
    }

    /// Open an existing on-disk index.
    pub fn open_dir(path: &Path) -> Result<Self> {
        let schema = TomeSchema::build();
        let dir = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| TomeError::Other(format!("open mmap dir: {e}")))?;
        let inner = TantivyIndex::open_or_create(dir, schema.schema.clone())
            .map_err(|e| TomeError::Other(format!("open index: {e}")))?;
        Ok(Self { inner, schema })
    }

    pub fn writer(&self, buffer_bytes: usize) -> Result<Writer> {
        let inner = self
            .inner
            .writer(buffer_bytes)
            .map_err(|e| TomeError::Other(format!("writer init: {e}")))?;
        Ok(Writer {
            inner,
            schema: self.schema.clone(),
        })
    }

    pub fn schema(&self) -> &TomeSchema {
        &self.schema
    }

    /// Run a query string against the index. `tier_filter` restricts results
    /// to articles in those tiers; an empty filter matches all tiers.
    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
        tier_filter: &[Tier],
    ) -> Result<Vec<SearchHit>> {
        let reader = self
            .inner
            .reader()
            .map_err(|e| TomeError::Other(format!("reader: {e}")))?;
        let searcher = reader.searcher();

        let parser = QueryParser::for_index(&self.inner, vec![self.schema.title, self.schema.body]);
        // A user typing into the search box can produce queries Tantivy's
        // parser rejects (unbalanced quotes, lone `:`, control chars from
        // paste). Surface those as "no results" rather than scary errors —
        // search-as-you-type is bad UX if every odd keystroke pops a stack
        // trace. Real I/O errors below are still propagated.
        let user_query: Box<dyn Query> = match parser.parse_query(query_str) {
            Ok(q) => q,
            Err(_) => return Ok(Vec::new()),
        };

        let final_query: Box<dyn Query> = if tier_filter.is_empty() {
            user_query
        } else {
            let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            clauses.push((Occur::Must, user_query));
            let tier_clause = build_tier_filter(self.schema.tier, tier_filter);
            clauses.push((Occur::Must, tier_clause));
            Box::new(BooleanQuery::new(clauses))
        };

        let top = searcher
            .search(&*final_query, &TopDocs::with_limit(limit))
            .map_err(|e| TomeError::Other(format!("execute search: {e}")))?;

        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher
                .doc(address)
                .map_err(|e| TomeError::Other(format!("read doc: {e}")))?;
            let page_id = read_u64(&doc, self.schema.page_id)?;
            let title = read_text(&doc, self.schema.title)?;
            let tier_str = read_text(&doc, self.schema.tier)?;
            let tier = parse_tier(&tier_str)?;
            hits.push(SearchHit {
                page_id,
                title,
                tier,
                score,
            });
        }
        Ok(hits)
    }
}

fn build_tier_filter(tier_field: tantivy::schema::Field, tiers: &[Tier]) -> Box<dyn Query> {
    let clauses: Vec<(Occur, Box<dyn Query>)> = tiers
        .iter()
        .map(|t| {
            let term = Term::from_field_text(tier_field, t.as_str());
            let q: Box<dyn Query> = Box::new(TermQuery::new(term, IndexRecordOption::Basic));
            (Occur::Should, q)
        })
        .collect();
    Box::new(BooleanQuery::new(clauses))
}

fn parse_tier(s: &str) -> Result<Tier> {
    match s {
        "hot" => Ok(Tier::Hot),
        "warm" => Ok(Tier::Warm),
        "cold" => Ok(Tier::Cold),
        "evicted" => Ok(Tier::Evicted),
        other => Err(TomeError::Other(format!("unknown tier in index: {other}"))),
    }
}

fn read_u64(doc: &TantivyDocument, field: tantivy::schema::Field) -> Result<u64> {
    use tantivy::schema::Value;
    doc.get_first(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| TomeError::Other("doc missing u64 field".into()))
}

fn read_text(doc: &TantivyDocument, field: tantivy::schema::Field) -> Result<String> {
    use tantivy::schema::Value;
    doc.get_first(field)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| TomeError::Other("doc missing text field".into()))
}

impl Searcher for Index {
    fn search(&self, query: &str, limit: usize, tier_filter: &[Tier]) -> Result<Vec<SearchHit>> {
        Index::search(self, query, limit, tier_filter)
    }
    fn name(&self) -> &str {
        "bm25"
    }
}

pub struct Writer {
    inner: IndexWriter,
    schema: TomeSchema,
}

impl Writer {
    /// Add a single article to the index. Multiple `add` calls between
    /// `commit`s are batched in memory.
    pub fn add(&self, page_id: u64, title: &str, body: &str, tier: Tier) -> Result<()> {
        let doc = doc!(
            self.schema.page_id => page_id,
            self.schema.title => title,
            self.schema.body => body,
            self.schema.tier => tier.as_str(),
        );
        self.inner
            .add_document(doc)
            .map_err(|e| TomeError::Other(format!("add document: {e}")))?;
        Ok(())
    }

    /// Flush the current batch to durable storage. Tantivy commits are
    /// expensive (segment merge); call this after each large batch, not
    /// per-document.
    pub fn commit(&mut self) -> Result<()> {
        self.inner
            .commit()
            .map_err(|e| TomeError::Other(format!("commit: {e}")))?;
        Ok(())
    }
}
