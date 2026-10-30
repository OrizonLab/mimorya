//! The documents of the in-memory ports: what a run may read (as the
//! platform filters it, space and clearance), found by shared words. For
//! tests only.

use async_trait::async_trait;
use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::{DocumentReadFilter, DocumentSpace};
use mimorya_core::memory::UnitFilter;
use uuid::Uuid;

use super::in_memory_memory::tokenize;
use super::in_memory_ports::InMemoryPorts;
use crate::runtime::{DocumentExcerpt, DocumentHit, DocumentPassage, DocumentStore};

/// A deposited document, its text already cut into passages.
#[derive(Debug, Clone)]
pub struct DocumentFixture {
    pub id: Uuid,
    pub title: String,
    pub space: DocumentSpace,
    pub sensitivity: Sensitivity,
    pub passages: Vec<String>,
}

impl DocumentFixture {
    /// An internal document of the company's space.
    pub fn new(title: &str, passages: &[&str]) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            space: DocumentSpace::Company,
            sensitivity: Sensitivity::Internal,
            passages: passages.iter().map(|p| (*p).to_string()).collect(),
        }
    }

    fn read_by(&self, filter: &DocumentReadFilter) -> bool {
        let space = match self.space {
            DocumentSpace::Company => filter.company,
            DocumentSpace::Unit(id) => match &filter.units {
                UnitFilter::All => true,
                UnitFilter::Only(units) => units.contains(&id),
            },
        };
        // Shared with the reader's units, up to its sensitivity (ADR-143).
        let shared = (filter.from_shares.iter())
            .any(|&(unit, cap)| self.space == DocumentSpace::Unit(unit) && self.sensitivity <= cap);
        (space && self.sensitivity <= filter.clearance) || shared
    }

    fn passage(&self, ordinal: usize) -> DocumentPassage {
        DocumentPassage {
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            page: None,
            heading: None,
            content: self.passages[ordinal].clone(),
        }
    }
}

#[async_trait]
impl DocumentStore for InMemoryPorts {
    async fn search(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>> {
        let terms = tokenize(query);
        let mut hits: Vec<(usize, DocumentHit)> = self
            .state()
            .documents
            .iter()
            .filter(|document| document.read_by(filter))
            .flat_map(|document| {
                (0..document.passages.len()).map(move |ordinal| (document, ordinal))
            })
            .filter_map(|(document, ordinal)| {
                let words = tokenize(&document.passages[ordinal]);
                let score = terms.iter().filter(|t| words.contains(t)).count();
                (score > 0).then(|| {
                    let hit = DocumentHit {
                        document_id: document.id,
                        title: document.title.clone(),
                        passage: document.passage(ordinal),
                    };
                    (score, hit)
                })
            })
            .collect();
        hits.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        hits.truncate(limit as usize);
        Ok(hits.into_iter().map(|(_, hit)| hit).collect())
    }

    async fn read(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>> {
        let state = self.state();
        let Some(document) = state
            .documents
            .iter()
            .find(|document| document.id == document_id && document.read_by(filter))
        else {
            return Ok(None);
        };
        let passages = (from as usize..document.passages.len())
            .take(count as usize)
            .map(|ordinal| document.passage(ordinal))
            .collect();
        Ok(Some(DocumentExcerpt {
            title: document.title.clone(),
            passages,
            total: u32::try_from(document.passages.len()).unwrap_or(u32::MAX),
        }))
    }
}
