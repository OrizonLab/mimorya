//! What a company may choose as its own model provider (ADR-024): a known
//! kind, a model name, and an endpoint only among those the operator lists.
//! A company never points the server at an address of its choosing (that
//! would let it reach the operator's internal network), and a key never moves
//! from one endpoint to another.

use super::anthropic::{DEFAULT_ANTHROPIC_MODEL, DEFAULT_BASE_URL};
use super::factory::{PROVIDER_KINDS, ProviderConfig, default_endpoint};
use super::openai_compatible::OPENAI_BASE_URL;

pub const MAX_MODEL_CHARS: usize = 100;
pub const MIN_API_KEY_CHARS: usize = 8;
pub const MAX_API_KEY_CHARS: usize = 500;

/// A company's provider, once checked; its key travels apart (a secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantLlmChoice {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
}

/// Kinds served at an endpoint the operator lists (a local or private
/// server); the others have a fixed public endpoint and need a key.
pub fn uses_listed_endpoint(kind: &str) -> bool {
    matches!(kind, "ollama" | "openai_compatible")
}

pub fn requires_api_key(kind: &str) -> bool {
    !uses_listed_endpoint(kind)
}

/// The model a kind uses when none is given, if it has one.
pub fn default_model(kind: &str) -> Option<&'static str> {
    (kind == "anthropic").then_some(DEFAULT_ANTHROPIC_MODEL)
}

/// The choice, trimmed, or why it is refused (in French).
pub fn check_tenant_llm(
    provider: &str,
    model: Option<&str>,
    base_url: Option<&str>,
    listed_endpoints: &[String],
) -> Result<TenantLlmChoice, String> {
    let provider = provider.trim();
    if !PROVIDER_KINDS.contains(&provider) {
        return Err(format!("Fournisseur inconnu : {provider}"));
    }
    let model = match model.map(str::trim).filter(|m| !m.is_empty()) {
        Some(model) => model,
        None => default_model(provider)
            .ok_or_else(|| format!("Indiquez le modèle à utiliser chez « {provider} »"))?,
    };
    check_model_name(model)?;
    let base_url = chosen_endpoint(
        provider,
        uses_listed_endpoint(provider),
        base_url,
        listed_endpoints,
    )?;
    Ok(TenantLlmChoice {
        provider: provider.to_string(),
        model: model.to_string(),
        base_url,
    })
}

/// Refuses a model name a provider would not take (in French).
pub(crate) fn check_model_name(model: &str) -> Result<(), String> {
    let well_formed = model.chars().count() <= MAX_MODEL_CHARS
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/@-".contains(c));
    if !well_formed {
        return Err(format!(
            "Nom de modèle invalide : lettres, chiffres et « . _ : / @ - », \
             {MAX_MODEL_CHARS} caractères au plus"
        ));
    }
    Ok(())
}

/// The endpoint a company chose: none for a kind served at its official
/// address, else one the operator lists (`listed_kind`); why not, in French.
pub(crate) fn chosen_endpoint(
    provider: &str,
    listed_kind: bool,
    base_url: Option<&str>,
    listed_endpoints: &[String],
) -> Result<Option<String>, String> {
    let base_url = base_url.map(str::trim).filter(|u| !u.is_empty());
    match (listed_kind, base_url) {
        (false, None) => Ok(None),
        (false, Some(_)) => Err(format!(
            "« {provider} » s’utilise à son adresse officielle : pas d’adresse à indiquer"
        )),
        (true, Some(url)) if listed_endpoints.iter().any(|listed| listed == url) => {
            Ok(Some(url.to_string()))
        }
        (true, _) => Err(
            "Choisissez l’adresse du serveur parmi celles qu’autorise l’exploitant de la \
             plateforme"
                .into(),
        ),
    }
}

/// The public endpoint of a hosted kind, written out so that nothing comes
/// from the server's environment (`ANTHROPIC_BASE_URL`…).
pub fn official_endpoint(kind: &str) -> Option<&'static str> {
    match kind {
        "anthropic" => Some(DEFAULT_BASE_URL),
        "openai" => Some(OPENAI_BASE_URL),
        "openrouter" | "gemini" => default_endpoint(kind),
        _ => None,
    }
}

/// The configuration a company's runs use, from what it stored, checked again
/// against the rules as they stand now (an endpoint the operator removed from
/// the list is refused) and with its endpoint and key written out: nothing is
/// taken from the server's environment (its endpoint or its own key). The
/// refusal is in French, for the company's administrators.
pub fn company_provider_config(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    api_key: Option<String>,
    listed_endpoints: &[String],
) -> Result<ProviderConfig, String> {
    let choice = check_tenant_llm(provider, Some(model), base_url, listed_endpoints)?;
    if requires_api_key(&choice.provider) && api_key.is_none() {
        return Err("Aucune clé d’API n’est enregistrée pour ce fournisseur".into());
    }
    let base_url = choice
        .base_url
        .or_else(|| official_endpoint(&choice.provider).map(str::to_string));
    Ok(ProviderConfig {
        kind: choice.provider,
        model: Some(choice.model),
        api_key,
        base_url,
    })
}

/// Why an API key is refused (in French), if it is.
pub fn api_key_problem(key: &str) -> Option<String> {
    let length = key.chars().count();
    if !(MIN_API_KEY_CHARS..=MAX_API_KEY_CHARS).contains(&length) {
        return Some(format!(
            "Clé d’API de {MIN_API_KEY_CHARS} à {MAX_API_KEY_CHARS} caractères"
        ));
    }
    // Sent in an HTTP header: printable ASCII only, no space.
    if !key.chars().all(|c| c.is_ascii_graphic()) {
        return Some("Clé d’API invalide : caractères visibles, sans espace".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed() -> Vec<String> {
        vec!["http://ollama.interne:11434/v1".to_string()]
    }

    #[test]
    fn accepts_known_kinds_at_their_endpoint() {
        assert_eq!(
            check_tenant_llm("anthropic", None, None, &[]),
            Ok(TenantLlmChoice {
                provider: "anthropic".into(),
                model: DEFAULT_ANTHROPIC_MODEL.into(),
                base_url: None,
            })
        );
        let local = check_tenant_llm(
            "ollama",
            Some(" llama3.3:70b "),
            Some("http://ollama.interne:11434/v1"),
            &listed(),
        )
        .unwrap();
        assert_eq!(local.model, "llama3.3:70b");
        assert_eq!(
            local.base_url.as_deref(),
            Some("http://ollama.interne:11434/v1")
        );
        assert!(
            check_tenant_llm("openrouter", Some("anthropic/claude-opus-5-5"), None, &[]).is_ok()
        );
        assert!(requires_api_key("openrouter") && !requires_api_key("ollama"));
    }

    #[test]
    fn refuses_unknown_kinds_free_endpoints_and_odd_models() {
        for (provider, model, base_url) in [
            ("mistral", Some("x"), None),
            ("openai", None, None),
            ("openai", Some("gpt 5"), None),
            ("openai", Some(&"m".repeat(MAX_MODEL_CHARS + 1)[..]), None),
            ("anthropic", None, Some("http://169.254.169.254/latest")),
            ("ollama", Some("llama"), None),
            ("ollama", Some("llama"), Some("http://localhost:5432")),
            (
                "openai_compatible",
                Some("m"),
                Some("http://ollama.interne:11434/v1/"),
            ),
        ] {
            assert!(
                check_tenant_llm(provider, model, base_url, &listed()).is_err(),
                "{provider} {model:?} {base_url:?}"
            );
        }
    }

    #[test]
    fn rechecks_what_was_stored_and_writes_the_endpoint_out() {
        let hosted = company_provider_config(
            "anthropic",
            "claude-opus-5-5",
            None,
            Some("k".repeat(10)),
            &[],
        )
        .unwrap();
        assert_eq!(hosted.base_url.as_deref(), Some(DEFAULT_BASE_URL));
        assert_eq!(
            company_provider_config("openrouter", "m", None, Some("k".repeat(10)), &[])
                .unwrap()
                .base_url
                .as_deref(),
            Some("https://openrouter.ai/api/v1")
        );
        // No key: never the server's own.
        assert!(company_provider_config("anthropic", "claude-opus-5-5", None, None, &[]).is_err());
        // An endpoint the operator no longer lists.
        let endpoint = "http://ollama.interne:11434/v1";
        assert!(
            company_provider_config("ollama", "llama", Some(endpoint), None, &listed()).is_ok()
        );
        assert!(company_provider_config("ollama", "llama", Some(endpoint), None, &[]).is_err());
    }

    #[test]
    fn checks_api_keys() {
        assert_eq!(api_key_problem("sk-ant-api03-abcdef"), None);
        for refused in [
            "court",
            "avec espace dedans",
            "sk-ant-\u{e9}t\u{e9}-0000",
            "a\nbcdefghij",
        ] {
            assert!(api_key_problem(refused).is_some(), "{refused}");
        }
        assert!(api_key_problem(&"k".repeat(MAX_API_KEY_CHARS + 1)).is_some());
    }
}
