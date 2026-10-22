//! An embedding model for tests: words of one concept share a dimension,
//! any other word gets one of its own (from a hash), so that a request finds
//! by meaning a memory it shares no word with. It records what it embedded,
//! fails on demand, refuses the texts holding a given word as a provider
//! refuses a text it cannot take, and holds its answer until released, to
//! act while a call is in flight. For tests only.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::Notify;

use crate::llm::embedding::{EMBEDDING_DIMENSIONS, Embedded, EmbeddingProvider};
use crate::llm::{LlmError, LlmErrorKind};

/// Dimensions of the concepts; words outside them hash to the others.
const CONCEPT_DIMENSIONS: usize = 64;

pub struct ConceptEmbedder {
    /// Words of each concept: a word belongs to one when it starts with one
    /// of them (plurals included).
    concepts: Vec<Vec<String>>,
    embedded: Mutex<Vec<String>>,
    calls: AtomicUsize,
    failing: Mutex<bool>,
    /// A request holding the word is refused so.
    refused: Mutex<Vec<(String, LlmErrorKind)>>,
    held: Mutex<bool>,
    entered: Notify,
    released: Notify,
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
            calls: AtomicUsize::new(0),
            failing: Mutex::new(false),
            refused: Mutex::new(Vec::new()),
            held: Mutex::new(false),
            entered: Notify::new(),
            released: Notify::new(),
        }
    }

    /// Every text embedded so far, in order.
    pub fn embedded(&self) -> Vec<String> {
        self.embedded.lock().expect("embedded").clone()
    }

    /// How many requests it received.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// Makes every following request fail (or succeed again).
    pub fn fail(&self, failing: bool) {
        *self.failing.lock().expect("failing") = failing;
    }

    /// Refuses every following request holding one of the words, with its
    /// kind of error (none: nothing is refused).
    pub fn refuse(&self, words: &[(&str, LlmErrorKind)]) {
        *self.refused.lock().expect("refused") = words
            .iter()
            .map(|(word, kind)| ((*word).to_string(), *kind))
            .collect();
    }

    /// Holds the following requests until `release`.
    pub fn hold(&self) {
        *self.held.lock().expect("held") = true;
    }

    pub fn release(&self) {
        *self.held.lock().expect("held") = false;
        self.released.notify_waiters();
    }

    /// Returns once a held request came in.
    pub async fn entered(&self) {
        self.entered.notified().await;
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

    fn is_held(&self) -> bool {
        *self.held.lock().expect("held")
    }

    async fn wait_while_held(&self) {
        if !self.is_held() {
            return;
        }
        self.entered.notify_one();
        loop {
            // Registered before the check: a release in between is not lost.
            let released = self.released.notified();
            if !self.is_held() {
                return;
            }
            released.await;
        }
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.wait_while_held().await;
        if *self.failing.lock().expect("failing") {
            return Err(LlmError::new(
                "test",
                LlmErrorKind::Overloaded,
                "Fournisseur indisponible",
            ));
        }
        let refused = self.refused.lock().expect("refused").clone();
        for (word, kind) in refused {
            if texts.iter().any(|t| t.contains(&word)) {
                return Err(LlmError::new(
                    "test",
                    kind,
                    format!("Texte refusé : {word}"),
                ));
            }
        }
        let vectors: Vec<Vec<f32>> = texts.iter().map(|t| self.vector(t)).collect();
        // As a provider's answer is checked: no vector without a word.
        if vectors.iter().any(|v| v.iter().all(|x| *x == 0.0)) {
            return Err(LlmError::new(
                "test",
                LlmErrorKind::InvalidRequest,
                "Texte sans mot",
            ));
        }
        self.embedded
            .lock()
            .expect("embedded")
            .extend(texts.iter().cloned());
        Ok(Embedded {
            vectors,
            input_tokens: texts
                .iter()
                .map(|t| t.split_whitespace().count() as u64)
                .sum(),
        })
    }
}
