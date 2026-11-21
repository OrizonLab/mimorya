//! The model services and the decision service the company's data go to,
//! named as the register says them: by their provider, or by the server
//! the operator designates and the API it speaks.

use super::facts::{ChosenBy, ModelService};
use super::recipients::{Place, Recipient};

/// A model service, for `role`: at the server it is reached at when that
/// is not its provider's own, named with the API it speaks (a gateway may
/// relay it further).
pub(super) fn model(service: &ModelService, role: &str) -> Recipient {
    let (name, place) = match (service.provider.as_str(), &service.server_host) {
        (kind, Some(host)) => (
            format!(
                "Serveur de modèles désigné par l’exploitant ({host}), {}",
                api(kind)
            ),
            Place::OperatorServer,
        ),
        ("anthropic", None) => ("Anthropic".into(), Place::OutsideEuropeanUnion),
        ("openai", None) => ("OpenAI".into(), Place::OutsideEuropeanUnion),
        ("openrouter", None) => (
            "OpenRouter, et le fournisseur du modèle vers lequel il route".into(),
            Place::OutsideEuropeanUnion,
        ),
        ("gemini", None) => ("Google (Gemini)".into(), Place::OutsideEuropeanUnion),
        ("mistral", None) => ("Mistral AI".into(), Place::EuropeanUnion),
        (other, None) => (format!("Fournisseur « {other} »"), Place::Unknown),
    };
    Recipient::new(name, role, place, service.chosen_by)
}

/// The API a model's kind speaks.
fn api(kind: &str) -> String {
    match kind {
        "anthropic" => "API Anthropic".into(),
        "openai" => "API OpenAI".into(),
        "openrouter" => "API OpenRouter".into(),
        "gemini" => "API Gemini".into(),
        "mistral" => "API Mistral".into(),
        "ollama" => "API Ollama".into(),
        "openai_compatible" => "API compatible avec OpenAI".into(),
        other => format!("API « {other} »"),
    }
}

/// TypeSafe's Jev, for `role` (ADR-213): on the company's key.
pub(super) fn jev(role: &str) -> Recipient {
    Recipient::new(
        "TypeSafe (Jev)",
        role,
        Place::OutsideEuropeanUnion,
        ChosenBy::Company,
    )
}
