//! What a company may choose as its embedding model (ADR-074), on the rules
//! of its model provider (ADR-024): a known kind, a model name, an endpoint
//! only among those the operator lists, a key that never moves from one
//! endpoint to another; and how close a memory must be to count.

use std::sync::Arc;

use super::EmbeddingProvider;
use super::provider::OpenAiEmbedder;
use crate::llm::openai_compatible::OPENAI_BASE_URL;
use crate::llm::settings::{check_model_name, chosen_endpoint};

pub const EMBEDDING_KINDS: [&str; 4] = ["mistral", "openai", "ollama", "openai_compatible"];
pub const MISTRAL_BASE_URL: &str = "https://api.mistral.ai/v1";

/// Kinds served at an endpoint the operator lists; the others have a fixed
/// public endpoint and need a key.
pub fn embedding_uses_listed_endpoint(kind: &str) -> bool {
    matches!(kind, "ollama" | "openai_compatible")
}

pub fn embedding_requires_key(kind: &str) -> bool {
    !embedding_uses_listed_endpoint(kind)
}

/// The model a kind uses when none is given, if it has one.
pub fn default_embedding_model(kind: &str) -> Option<&'static str> {
    match kind {
        "mistral" => Some("mistral-embed"),
        "openai" => Some("text-embedding-3-small"),
        _ => None,
    }
}

/// Where to start the closeness a memory needs, for a kind: models spread
/// their similarities differently (Mistral's sit high, OpenAI's low). A
/// starting value, to adjust to the model.
pub fn default_min_similarity(kind: &str) -> f32 {
    match kind {
        "mistral" => 0.75,
        "openai" => 0.35,
        _ => 0.5,
    }
}

fn official_endpoint(kind: &str) -> Option<&'static str> {
    match kind {
        "mistral" => Some(MISTRAL_BASE_URL),
        "openai" => Some(OPENAI_BASE_URL),
        _ => None,
    }
}

/// A company's embedding model, once checked; its key travels apart.
#[derive(Debug, Clone, PartialEq)]
pub struct TenantEmbeddingChoice {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub min_similarity: f32,
}

/// The choice, trimmed and completed, or why it is refused (in French).
pub fn check_tenant_embedding(
    provider: &str,
    model: Option<&str>,
    base_url: Option<&str>,
    min_similarity: Option<f32>,
    listed_endpoints: &[String],
) -> Result<TenantEmbeddingChoice, String> {
    let provider = provider.trim();
    if !EMBEDDING_KINDS.contains(&provider) {
        return Err(format!("Fournisseur d’embeddings inconnu : {provider}"));
    }
    let model = match model.map(str::trim).filter(|m| !m.is_empty()) {
        Some(model) => model,
        None => default_embedding_model(provider).ok_or_else(|| {
            format!("Indiquez le modèle d’embeddings à utiliser chez « {provider} »")
        })?,
    };
    check_model_name(model)?;
    let listed_kind = embedding_uses_listed_endpoint(provider);
    let base_url = chosen_endpoint(provider, listed_kind, base_url, listed_endpoints)?;
    let min_similarity = min_similarity.unwrap_or_else(|| default_min_similarity(provider));
    if !(min_similarity > 0.0 && min_similarity < 1.0) {
        return Err("La similarité minimale est comprise strictement entre 0 et 1".into());
    }
    Ok(TenantEmbeddingChoice {
        provider: provider.to_string(),
        model: model.to_string(),
        base_url,
        min_similarity,
    })
}

#[derive(Clone, PartialEq, Eq)]
pub struct EmbeddingConfig {
    /// One of `EMBEDDING_KINDS`.
    pub kind: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: String,
}

/// The API key never appears in debug output (logs, panics).
impl std::fmt::Debug for EmbeddingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingConfig")
            .field("kind", &self.kind)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// The configuration a company's memory is embedded with, from what it
/// stored, checked again against the rules as they stand now (an endpoint
/// the operator removed from the list is refused) and with its endpoint and
/// key written out: nothing comes from the server's environment. The
/// refusal is in French, for the company's administrators.
pub fn company_embedding_config(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    api_key: Option<String>,
    listed_endpoints: &[String],
) -> Result<EmbeddingConfig, String> {
    let choice = check_tenant_embedding(provider, Some(model), base_url, None, listed_endpoints)?;
    if embedding_requires_key(&choice.provider) && api_key.is_none() {
        return Err("Aucune clé d’API n’est enregistrée pour ce fournisseur".into());
    }
    let base_url = choice
        .base_url
        .or_else(|| official_endpoint(&choice.provider).map(str::to_string))
        .ok_or_else(|| "Aucune adresse pour ce fournisseur".to_string())?;
    Ok(EmbeddingConfig {
        kind: choice.provider,
        model: choice.model,
        api_key,
        base_url,
    })
}

pub fn create_embedder(config: &EmbeddingConfig) -> anyhow::Result<Arc<dyn EmbeddingProvider>> {
    if !EMBEDDING_KINDS.contains(&config.kind.as_str()) {
        anyhow::bail!("Unknown embedding provider \"{}\"", config.kind);
    }
    Ok(Arc::new(OpenAiEmbedder::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTED: &str = "http://ollama.test:11434/v1";

    fn check(
        provider: &str,
        model: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<TenantEmbeddingChoice, String> {
        check_tenant_embedding(provider, model, base_url, None, &[LISTED.to_string()])
    }

    #[test]
    fn hosted_kinds_take_their_default_model_and_no_address() {
        let mistral = check(" mistral ", None, None).unwrap();
        assert_eq!(mistral.model, "mistral-embed");
        assert_eq!(mistral.base_url, None);
        assert_eq!(mistral.min_similarity, 0.75);
        assert!(check("openai", None, Some(LISTED)).is_err());
        assert!(check("anthropic", None, None).is_err());
    }

    #[test]
    fn a_served_kind_names_its_model_and_a_listed_address() {
        assert!(check("ollama", None, Some(LISTED)).is_err());
        assert!(check("ollama", Some("bge-m3"), None).is_err());
        assert!(check("ollama", Some("bge-m3"), Some("http://169.254.169.254/v1")).is_err());
        let ollama = check("ollama", Some("bge-m3"), Some(LISTED)).unwrap();
        assert_eq!(ollama.base_url.as_deref(), Some(LISTED));
        assert!(check("ollama", Some("bge m3"), Some(LISTED)).is_err());
    }

    #[test]
    fn the_similarity_lies_strictly_between_zero_and_one() {
        let listed = [LISTED.to_string()];
        let with = |s| check_tenant_embedding("openai", None, None, Some(s), &listed);
        assert_eq!(with(0.4).unwrap().min_similarity, 0.4);
        assert!(with(0.0).is_err());
        assert!(with(1.0).is_err());
        assert!(with(f32::NAN).is_err());
    }

    #[test]
    fn the_stored_choice_is_checked_again_and_written_out() {
        let listed = [LISTED.to_string()];
        let key = Some("sk-cle-de-test".to_string());
        let mistral = company_embedding_config("mistral", "mistral-embed", None, key, &listed);
        assert_eq!(mistral.unwrap().base_url, MISTRAL_BASE_URL);
        let keyless =
            company_embedding_config("openai", "text-embedding-3-small", None, None, &listed);
        assert!(keyless.is_err());
        let removed = company_embedding_config("ollama", "bge-m3", Some(LISTED), None, &[]);
        assert!(removed.is_err());
        let debug = format!(
            "{:?}",
            company_embedding_config(
                "mistral",
                "mistral-embed",
                None,
                Some("sk-secret".into()),
                &listed
            )
            .unwrap()
        );
        assert!(!debug.contains("sk-secret"), "{debug}");
    }
}
