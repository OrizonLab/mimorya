//! What administrators see of a connector that cannot be used: its token
//! opened from the secret store or why it cannot be, and the recorded
//! message of a failed start (bounded, the token masked once more).

use mimorya_connectors::outbound::Redactor;

use crate::secrets::SecretRead;

const MAX_ERROR_CHARS: usize = 300;

/// The token of an instance, or why it cannot be used.
pub fn opened(read: SecretRead) -> anyhow::Result<Option<String>> {
    match read {
        SecretRead::Absent => Ok(None),
        SecretRead::Value(token) => Ok(Some(token)),
        SecretRead::NoMasterKey => {
            anyhow::bail!("Ce serveur n’a pas de clé maîtresse : le jeton ne peut pas être lu")
        }
        SecretRead::Unreadable => {
            anyhow::bail!(
                "Le jeton enregistré ne peut pas être déchiffré : saisissez-le de nouveau"
            )
        }
    }
}

/// The recorded message of a failed start. Connectors explain their own
/// failures in French; the token is masked again whatever the connector did.
pub fn failure(error: &anyhow::Error, secret: Option<&str>) -> String {
    Redactor::new(secret)
        .text(&format!("{error:#}"))
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_failure_is_bounded_and_never_shows_the_token() {
        let error = anyhow::anyhow!("Accès refusé : jeton jeton-secret-123 refusé");
        let shown = failure(&error, Some("jeton-secret-123"));
        assert_eq!(shown, "Accès refusé : jeton *** refusé");
        let long = anyhow::anyhow!("{}", "é".repeat(1000));
        assert_eq!(failure(&long, None).chars().count(), MAX_ERROR_CHARS);
        assert!(opened(SecretRead::NoMasterKey).is_err());
        assert!(opened(SecretRead::Unreadable).is_err());
        assert_eq!(opened(SecretRead::Absent).unwrap(), None);
    }
}
