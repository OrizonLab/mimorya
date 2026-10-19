//! How the server reaches its SMTP relay. The URL may carry credentials:
//! they are never printed, and only travel over TLS (`smtps://`, or
//! `smtp://…?tls=required` for STARTTLS).

use std::fmt;

use lettre::message::Mailbox;
use url::Url;

#[derive(Clone)]
pub struct MailSettings {
    pub smtp_url: String,
    pub from: Mailbox,
}

impl MailSettings {
    /// The settings, or the variable at fault and why (in French).
    pub fn new(smtp_url: &str, from: &str) -> Result<Self, (&'static str, String)> {
        let url = Url::parse(smtp_url)
            .map_err(|_| ("SMTP_URL", "URL smtp:// ou smtps:// attendue".to_string()))?;
        // The first `tls` parameter, as the SMTP library reads it.
        let tls = url.scheme() == "smtps"
            || url
                .query_pairs()
                .find(|(key, _)| key == "tls")
                .is_some_and(|(_, value)| value == "required");
        if !matches!(url.scheme(), "smtp" | "smtps") || url.host_str().is_none_or(str::is_empty) {
            return Err(("SMTP_URL", "URL smtp:// ou smtps:// attendue".into()));
        }
        if (!url.username().is_empty() || url.password().is_some()) && !tls {
            return Err((
                "SMTP_URL",
                "identifiants seulement sur TLS : smtps:// ou ?tls=required".into(),
            ));
        }
        let from: Mailbox = from.parse().map_err(|_| {
            (
                "MAIL_FROM",
                "adresse d’expéditeur invalide (ex. Mimorya <no-reply@exemple.fr>)".to_string(),
            )
        })?;
        Ok(Self {
            smtp_url: smtp_url.to_string(),
            from,
        })
    }
}

/// The relay's host only: the URL may carry credentials.
impl fmt::Debug for MailSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = Url::parse(&self.smtp_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string));
        f.debug_struct("MailSettings")
            .field("smtp_host", &host)
            .field("from", &self.from.to_string())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: &str = "Mimorya <no-reply@exemple.fr>";

    #[test]
    fn credentials_travel_over_tls_only_and_are_never_printed() {
        for accepted in [
            "smtp://localhost:1025",
            "smtps://user:s3cret@smtp.exemple.fr:465",
            "smtp://user:s3cret@smtp.exemple.fr:587?tls=required",
        ] {
            let settings = MailSettings::new(accepted, FROM).unwrap();
            assert!(!format!("{settings:?}").contains("s3cret"));
        }
        for refused in [
            "smtp://user:s3cret@smtp.exemple.fr:587",
            "smtp://user:s3cret@smtp.exemple.fr:587?tls=opportunistic",
            "smtp://user:s3cret@smtp.exemple.fr:587?tls=opportunistic&tls=required",
            "http://smtp.exemple.fr",
            "smtp://",
            "pas une adresse",
        ] {
            assert_eq!(MailSettings::new(refused, FROM).unwrap_err().0, "SMTP_URL");
        }
        assert_eq!(
            MailSettings::new("smtp://localhost", "pas une adresse")
                .unwrap_err()
                .0,
            "MAIL_FROM"
        );
    }
}
