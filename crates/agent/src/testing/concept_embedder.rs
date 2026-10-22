//! An embedding model for tests: words of one concept share a dimension,
//! any other word gets one of its own (from a hash), so that a request finds
//! by meaning a memory it shares no word with. It records what it embedded,
//! fails on demand, and refuses the texts holding a given word, as a
//! provider refuses a text it cannot take. For tests only.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::llm::embedding::{EMBEDDING_DIMENSIONS, Embedded, EmbeddingProvider};
use crate::llm::{LlmError, LlmErrorKind};

/// Dimensions of the concepts; words outside them hash to the others.
const CONCEPT_DIMENSIONS: usize = 64;

pub struct ConceptEmbedder {
    /// Words of each concept: a word belongs to one when it starts with one
    /// of them (plurals included).
    concepts: Vec<Vec<String>>,
    embedded: Mutex<Vec<String>>,
    failing: Mutex<bool>,
    refused: Mutex<Option<String>>,
}

impl ConceptEmbedder {
    pub fn new(concepts: &[&[&str]]) -> Self {
        assert!(concepts.len() <= CONCEPT_DIMENSIONS, "too many concepts");
        Self {
            concepts: concepts
                .iter()
                .map(|words| words.iter().map(|w| w.to_lowercase()).collect())
                .collect(),
            embedded: Mutex::new(Vec::new()),
            failing: Mutex::new(false),
            refused: Mutex::new(None),
        }
    }

    /// Every text embedded so far, in order.
    pub fn embedded(&self) -> Vec<String> {
        self.embedded.lock().expect("embedded").clone()
    }

    /// Makes every following request fail (or succeed again).
    pub fn fail(&self, failing: bool) {
        *self.failing.lock().expect("failing") = failing;
    }

    /// Makes every following request holding `word` refused (or none).
    pub fn refuse(&self, word: Option<&str>) {
        *self.refused.lock().expect("refused") = word.map(str::to_string);
    }

    pub fn vector(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0; EMBEDDING_DIMENSIONS];
        let lowered = text.to_lowercase();
        let words = lowered
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() > 2);
        for word in words {
            let concept = self
                .concepts
                .iter()
                .position(|words| words.iter().any(|w| word.starts_with(w.as_str())));
            let dimension = concept.unwrap_or_else(|| {
                // FNV-1a: stable from one run to the next.
                let hash = word.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
                });
                let spread = (EMBEDDING_DIMENSIONS - CONCEPT_DIMENSIONS) as u64;
                CONCEPT_DIMENSIONS + usize::try_from(hash % spread).unwrap_or(0)
            });
            vector[dimension] += 1.0;
        }
        vector
    }
}

#[async_trait]
impl EmbeddingProvider for ConceptEmbedder {
    fn id(&self) -> &str {
        "test"
    }

    fn model(&self) -> &str {
        "concepts"
    }

    async fn embed(&self, texts: &[String]) -> Result<Embedded, LlmError> {
        if *self.failing.lock().expect("failing") {
            return Err(LlmError::new(
                "test",
                LlmErrorKind::Overloaded,
                "Fournisseur indisponible",
            ));
        }
        let refused = self.refused.lock().expect("refused").clone();
        if refused.is_some_and(|word| texts.iter().any(|t| t.contains(&word))) {
            return Err(LlmError::new(
                "test",
                LlmErrorKind::InvalidRequest,
                "Texte refusé",
            ));
        }
        self.embedded
            .lock()
            .expect("embedded")
            .extend(texts.iter().cloned());
        Ok(Embedded {
            vectors: texts.iter().map(|t| self.vector(t)).collect(),
            input_tokens: texts
                .iter()
                .map(|t| t.split_whitespace().count() as u64)
                .sum(),
        })
    }
}
