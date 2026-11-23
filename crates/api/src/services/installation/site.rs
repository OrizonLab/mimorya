//! The platform's own address, reached as anyone reaches it: the domain
//! resolves to this server, its ports answer, its certificate is one
//! browsers trust. Reached directly, never through the models' proxy.

use std::time::Duration;

use super::{Finding, State};
use crate::config::AppConfig;

const TOPIC: &str = "Domaine et certificat";
const TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn reachable(config: &AppConfig, web: &reqwest_mcp::Client) -> Finding {
    let origin = config.web_origin.as_str();
    let health = format!("{origin}/api/health");
    let answer = web.get(&health).timeout(TIMEOUT).send().await;
    said(origin, answer.map(|response| response.status().as_u16()))
}

/// What reaching `origin` gave: its status, or why it could not.
pub(super) fn said(origin: &str, answer: Result<u16, reqwest_mcp::Error>) -> Finding {
    match answer {
        Ok(status) if (200..300).contains(&status) && origin.starts_with("https://") => {
            Finding::new(
                TOPIC,
                State::Ready,
                format!("{origin} répond, son certificat est reconnu"),
            )
        }
        Ok(status) if (200..300).contains(&status) => Finding::new(
            TOPIC,
            State::ToComplete,
            format!(
                "{origin} répond, mais sans HTTPS : en production, MIMORYA_DOMAIN et une \
                 adresse e-mail dans MIMORYA_TLS donnent un certificat public (derrière un \
                 reverse proxy déjà installé, c’est lui qui sert HTTPS)"
            ),
        ),
        Ok(status) => Finding::new(
            TOPIC,
            State::Failed,
            format!(
                "{origin}/api/health répond {status} : la pile est-elle entière ? (docker compose … ps)"
            ),
        ),
        Err(error) => Finding::new(TOPIC, State::Failed, unreached(origin, &error)),
    }
}

fn unreached(origin: &str, error: &reqwest_mcp::Error) -> String {
    let mut cause = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        cause = inner.to_string();
        source = inner.source();
    }
    if cause.to_ascii_lowercase().contains("certificate") {
        return format!(
            "certificat de {origin} non reconnu ({cause}) : MIMORYA_TLS=internal ne sert qu’aux \
             essais ; une adresse e-mail obtient un certificat public (derrière un reverse \
             proxy déjà installé, c’est son certificat qui compte)"
        );
    }
    format!(
        "{origin} injoignable depuis le serveur ({cause}) : le DNS de MIMORYA_DOMAIN \
         pointe-t-il ici, les ports 80 et 443 sont-ils ouverts ? Essayez aussi depuis un poste \
         extérieur : curl -I {origin}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_said_as_the_operator_reads_it() {
        let secure = said("https://beta.example.com", Ok(200));
        assert_eq!(secure.state, State::Ready);
        let plain = said("http://localhost:3000", Ok(200));
        assert_eq!(plain.state, State::ToComplete);
        assert!(plain.said.contains("sans HTTPS"));
        let broken = said("https://beta.example.com", Ok(502));
        assert_eq!(broken.state, State::Failed);
        assert!(broken.said.contains("répond 502"));
    }
}
