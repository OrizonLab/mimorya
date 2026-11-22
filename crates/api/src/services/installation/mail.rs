//! The mail relay: configured or not, and a test e-mail to the address the
//! operator gives, through the relay the platform sends with.

use std::sync::Arc;

use super::{Finding, State};
use crate::config::AppConfig;
use crate::mail::{Mail, Mailer};

const TOPIC: &str = "Relais SMTP";

pub(super) async fn relay(
    config: &AppConfig,
    mailer: Option<Arc<dyn Mailer>>,
    to: Option<&str>,
    product: &str,
) -> Finding {
    let Some(settings) = &config.mail else {
        return Finding::new(
            TOPIC,
            State::ToComplete,
            "aucun (SMTP_URL, MAIL_FROM) : les invitations, les liens de mot de passe oublié \
             et les avis ne partent pas ; l’administrateur transmet lui-même les liens",
        );
    };
    let host = relay_host(&settings.smtp_url);
    let (Some(mailer), Some(to)) = (mailer, to) else {
        return Finding::new(
            TOPIC,
            State::ToComplete,
            format!(
                "{host}, expéditeur {} ; non essayé : --mail-to <adresse> envoie un e-mail \
                 d’essai",
                settings.from
            ),
        );
    };
    let mail = Mail {
        to: to.to_string(),
        to_name: to.to_string(),
        subject: format!("Essai d’envoi de {product}"),
        text: format!(
            "Bonjour,\n\nCet e-mail vérifie que {product} peut envoyer ses invitations, ses \
             liens de mot de passe oublié et ses avis. Il n’appelle aucune réponse.\n"
        ),
    };
    match mailer.send(&mail).await {
        Ok(()) => Finding::new(
            TOPIC,
            State::Ready,
            format!(
                "{host} a accepté l’e-mail d’essai pour {to} : vérifiez qu’il arrive, et pas \
                 dans les indésirables"
            ),
        ),
        Err(error) => Finding::new(TOPIC, State::Failed, format!("{host} : {error:#}")),
    }
}

/// The relay's host and port, never its credentials.
pub(super) fn relay_host(smtp_url: &str) -> String {
    let Ok(url) = url::Url::parse(smtp_url) else {
        return "relais illisible".to_string();
    };
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        _ => "relais sans hôte".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::relay_host;

    #[test]
    fn the_relay_is_named_without_its_credentials() {
        let named = relay_host("smtps://ops:Secret-2026@smtp.example.com:465");
        assert_eq!(named, "smtp.example.com:465");
        assert_eq!(relay_host("smtp://localhost"), "localhost");
        assert_eq!(relay_host("pas une adresse"), "relais illisible");
    }
}
