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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use mimorya_agent::llm::{LlmError, LlmResponse};

    use super::*;

    /// A model that never answers.
    struct Silent;

    #[async_trait]
    impl LlmProvider for Silent {
        fn id(&self) -> &str {
            "anthropic"
        }

        fn model(&self) -> &str {
            "claude-sonnet-5-5"
        }

        async fn complete(&self, _: &LlmRequest) -> Result<LlmResponse, LlmError> {
            std::future::pending().await
        }
    }

    /// The clock is tokio's, paused: the minute passes at once.
    #[tokio::test(start_paused = true)]
    async fn a_model_that_does_not_answer_fails_the_check_after_its_minute() {
        let config = AppConfig::from_vars(|key| match key {
            "DATABASE_APP_URL" => Some("postgres://app:app@localhost/mimorya".into()),
            "ANTHROPIC_API_KEY" => Some("cle-de-test".into()),
            _ => None,
        })
        .unwrap();
        let started = tokio::time::Instant::now();
        let found = platform(&config, Some(Arc::new(Silent))).await;
        assert_eq!(started.elapsed(), TIMEOUT);
        assert_eq!(found.state, State::Failed);
        assert_eq!(
            found.said,
            "anthropic, claude-sonnet-5-5 n’a pas répondu en 60 s"
        );
    }
}
