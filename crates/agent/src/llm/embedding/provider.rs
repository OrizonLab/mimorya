//! Adapter for OpenAI-compatible `/embeddings` endpoints: OpenAI, Mistral,
//! and the servers an operator lists (Ollama, vLLM, Text Embeddings
//! Inference…). Only OpenAI's own endpoint is asked for 1024 dimensions
//! (its models shorten their vectors on demand); the others must produce
//! them, or their answer is refused rather than stored truncated.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::settings::EmbeddingConfig;
use super::{EMBEDDING_DIMENSIONS, Embedded, EmbeddingProvider, MAX_EMBEDDING_BATCH};
use crate::llm::openai_compatible::errors::{from_status, malformed, network};
use crate::llm::types::{LlmError, LlmErrorKind};

/// No `Debug`: it would print the API key.
pub struct OpenAiEmbedder {
    http: reqwest::Client,
    id: String,
    model: String,
    api_key: String,
    base_url: String,
}

impl OpenAiEmbedder {
    pub fn new(config: &EmbeddingConfig) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("HTTP client"),
            id: config.kind.clone(),
            model: config.model.clone(),
            // Local servers accept any key.
            api_key: config
                .api_key
                .clone()
                .unwrap_or_else(|| "not-needed".into()),
            base_url: config.base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn build_body(&self, texts: &[String]) -> Value {
        let mut body = json!({ "model": self.model, "input": texts });
        if self.id == "openai" {
            body["dimensions"] = json!(EMBEDDING_DIMENSIONS);
        }
        body
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbedder {
    fn id(&self) -> &str {
        &self.id
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn embed(&self, texts: &[String]) -> Result<Embedded, LlmError> {
        if texts.len() > MAX_EMBEDDING_BATCH {
            return Err(LlmError::new(
                &self.id,
                LlmErrorKind::InvalidRequest,
                format!("{MAX_EMBEDDING_BATCH} textes au plus par demande"),
            ));
        }
        if texts.is_empty() {
            return Ok(Embedded {
                vectors: Vec::new(),
                input_tokens: 0,
            });
        }
        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&self.build_body(texts))
            .send()
            .await
            .map_err(|_| network(&self.id))?;
        let status = response.status();
        let body = response.text().await.map_err(|_| network(&self.id))?;
        if !status.is_success() {
            return Err(from_status(&self.id, status.as_u16(), &body));
        }
        parse_embeddings(&self.id, &body, texts.len())
    }
}

/// The vectors of an `/embeddings` answer for `count` texts, in the order
/// sent; refused when one is missing, of other dimensions, or not finite
/// (the database would refuse it). `InvalidRequest` is kept for what the
/// provider refuses in the texts sent.
pub fn parse_embeddings(provider: &str, body: &str, count: usize) -> Result<Embedded, LlmError> {
    let answer: Value = serde_json::from_str(body).map_err(|_| malformed(provider))?;
    let data = answer["data"]
        .as_array()
        .ok_or_else(|| malformed(provider))?;
    let mut vectors: Vec<Option<Vec<f32>>> = vec![None; count];
    for item in data {
        let index = item["index"].as_u64().and_then(|i| usize::try_from(i).ok());
        let slot = index
            .and_then(|i| vectors.get_mut(i))
            .ok_or_else(|| malformed(provider))?;
        let values = item["embedding"]
            .as_array()
            .ok_or_else(|| malformed(provider))?;
        if values.len() != EMBEDDING_DIMENSIONS {
            // The model's, not the text's: no memory is to blame.
            return Err(LlmError::new(
                provider,
                LlmErrorKind::MalformedOutput,
                format!(
                    "Le modèle renvoie des vecteurs de {} dimensions : {EMBEDDING_DIMENSIONS} \
                     sont attendues",
                    values.len()
                ),
            ));
        }
        let vector: Option<Vec<f32>> = values
            .iter()
            .map(|v| v.as_f64().map(|v| v as f32).filter(|v| v.is_finite()))
            .collect();
        *slot = Some(vector.ok_or_else(|| malformed(provider))?);
    }
    let vectors: Option<Vec<Vec<f32>>> = vectors.into_iter().collect();
    Ok(Embedded {
        vectors: vectors.ok_or_else(|| malformed(provider))?,
        input_tokens: answer["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(vectors: &[(usize, Vec<f32>)]) -> String {
        let data: Vec<Value> = vectors
            .iter()
            .map(|(index, v)| json!({ "object": "embedding", "index": index, "embedding": v }))
            .collect();
        json!({ "data": data, "usage": { "prompt_tokens": 7, "total_tokens": 7 } }).to_string()
    }

    fn vector(value: f32) -> Vec<f32> {
        vec![value; EMBEDDING_DIMENSIONS]
    }

    fn embedder(kind: &str) -> OpenAiEmbedder {
        OpenAiEmbedder::new(&EmbeddingConfig {
            kind: kind.into(),
            model: "m".into(),
            api_key: None,
            base_url: "http://embeddings.test/v1/".into(),
        })
    }

    #[test]
    fn vectors_come_back_in_the_order_sent() {
        let body = answer(&[(1, vector(0.5)), (0, vector(0.25))]);
        let embedded = parse_embeddings("mistral", &body, 2).unwrap();
        assert_eq!(embedded.vectors, [vector(0.25), vector(0.5)]);
        assert_eq!(embedded.input_tokens, 7);
    }

    #[test]
    fn a_missing_vector_or_one_of_other_dimensions_is_refused() {
        let missing = answer(&[(0, vector(0.5))]);
        let error = parse_embeddings("ollama", &missing, 2).unwrap_err();
        assert_eq!(error.kind, LlmErrorKind::MalformedOutput);
        let short = answer(&[(0, vec![0.5; 768])]);
        let error = parse_embeddings("ollama", &short, 1).unwrap_err();
        assert_eq!(error.kind, LlmErrorKind::MalformedOutput);
        assert_eq!(
            error.message,
            "Le modèle renvoie des vecteurs de 768 dimensions : 1024 sont attendues"
        );
        let outside = answer(&[(3, vector(0.5))]);
        assert!(parse_embeddings("ollama", &outside, 1).is_err());
        assert!(parse_embeddings("ollama", "pas du JSON", 1).is_err());
    }

    #[test]
    fn only_openai_is_asked_for_the_dimensions() {
        let texts = ["un".to_string()];
        let openai = embedder("openai").build_body(&texts);
        assert_eq!(openai["dimensions"], json!(EMBEDDING_DIMENSIONS));
        assert_eq!(openai["input"], json!(["un"]));
        let mistral = embedder("mistral").build_body(&texts);
        assert_eq!(mistral, json!({ "model": "m", "input": ["un"] }));
    }

    #[tokio::test]
    async fn more_texts_than_a_batch_are_refused_before_anything_is_sent() {
        let texts = vec!["texte".to_string(); MAX_EMBEDDING_BATCH + 1];
        let error = embedder("ollama").embed(&texts).await.unwrap_err();
        assert_eq!(error.kind, LlmErrorKind::InvalidRequest);
    }
}
