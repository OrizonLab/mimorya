//! The platform's model: configured or not and, when the operator asks, a
//! real call that proves its key: billed, its answer bounded to a few
//! tokens at Anthropic; an OpenAI-compatible provider takes no bound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mimorya_agent::llm::{LlmMessage, LlmProvider, LlmRequest};

use super::{Finding, State};
use crate::config::AppConfig;

const TOPIC: &str = "Modèle de la plateforme";
const TIMEOUT: Duration = Duration::from_secs(60);

pub(super) async fn platform(config: &AppConfig, called: Option<Arc<dyn LlmProvider>>) -> Finding {
    let Some(llm) = &config.llm else {
        return Finding::new(
            TOPIC,
            State::ToComplete,
            "aucun (ANTHROPIC_API_KEY ou LLM_PROVIDER) : chaque entreprise saisit sa propre \
             clé (formule « clé du client »)",
        );
    };
    let Some(provider) = called else {
        let model = llm.model.as_deref().unwrap_or("modèle par défaut");
        return Finding::new(
            TOPIC,
            State::ToComplete,
            format!(
                "{}, {model} ; non appelé : --call-model le vérifie par un vrai appel \
                 (quelques jetons, facturés)",
                llm.kind
            ),
        );
    };
    let named = format!("{}, {}", provider.id(), provider.model());
    let request = LlmRequest {
        system: "Tu vérifies une installation. Réponds seulement : OK.".to_string(),
        messages: vec![LlmMessage::user("Vérification de l’installation.")],
        tools: Vec::new(),
        max_output_tokens: Some(16),
        effort: None,
        on_text_delta: None,
        advisor: None,
        drop_unbound_thinking: false,
    };
    let started = Instant::now();
    match tokio::time::timeout(TIMEOUT, provider.complete(&request)).await {
        Ok(Ok(answer)) => Finding::new(
            TOPIC,
            State::Ready,
            format!(
                "{named} a répondu en {:.1} s ({} jetons lus, {} écrits)",
                started.elapsed().as_secs_f64(),
                answer.usage.input_tokens,
                answer.usage.output_tokens
            ),
        ),
        Ok(Err(error)) => Finding::new(TOPIC, State::Failed, format!("{named} : {error}")),
        Err(_) => Finding::new(
            TOPIC,
            State::Failed,
            format!("{named} n’a pas répondu en {} s", TIMEOUT.as_secs()),
        ),
    }
}
