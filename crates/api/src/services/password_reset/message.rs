//! The reset e-mail, in French and plain text: which account, who asked
//! (the person or an administrator), the link and until when it works, and
//! what to do if the request was unexpected.

use chrono::{DateTime, Utc};

use crate::mail::{Mail, expiry_label};

pub struct ResetMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub name: &'a str,
    pub email: &'a str,
    pub link: &'a str,
    pub expires_at: DateTime<Utc>,
    /// The administrator who sent the link; `None` when asked for from the
    /// login page.
    pub sent_by: Option<&'a str>,
}

pub fn reset_mail(reset: &ResetMail<'_>) -> Mail {
    let ResetMail {
        product,
        company,
        name,
        email,
        link,
        expires_at,
        sent_by,
    } = reset;
    let (asked, unexpected) = match sent_by {
        Some(administrator) => (
            format!(
                "{administrator} vous envoie ce lien pour réinitialiser le mot de passe de votre \
                 compte {product} ({company})."
            ),
            format!(
                "Votre mot de passe actuel reste valable tant que vous n’en avez pas choisi un \
                 nouveau. En cas de doute, contactez {administrator}."
            ),
        ),
        None => (
            format!(
                "La réinitialisation du mot de passe de votre compte {product} ({company}) a été \
                 demandée."
            ),
            "Si vous n’êtes pas à l’origine de cette demande, ignorez ce message : votre mot de \
             passe reste inchangé."
                .to_string(),
        ),
    };
    Mail {
        to: email.to_string(),
        to_name: name.to_string(),
        subject: format!("Réinitialisation de votre mot de passe {product}"),
        text: format!(
            "Bonjour {name},\n\n\
             {asked}\n\n\
             Pour choisir un nouveau mot de passe, ouvrez ce lien :\n\
             {link}\n\n\
             Ce lien est personnel et ne sert qu’une fois. Il expire le {}. Une fois le \
             nouveau mot de passe choisi, toutes vos autres sessions seront fermées.\n\n\
             {unexpected}\n",
            expiry_label(*expires_at)
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn mail(sent_by: Option<&str>) -> Mail {
        reset_mail(&ResetMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            name: "Zoé Leroy",
            email: "zoe.leroy@neo-maintenance.example",
            link: "https://app.exemple.fr/reset-password#jeton",
            expires_at: Utc.with_ymd_and_hms(2026, 10, 26, 5, 42, 0).unwrap(),
            sent_by,
        })
    }

    #[test]
    fn names_the_account_the_link_and_its_expiry() {
        let mail = mail(None);
        assert_eq!(mail.to, "zoe.leroy@neo-maintenance.example");
        assert_eq!(mail.to_name, "Zoé Leroy");
        assert_eq!(
            mail.subject,
            "Réinitialisation de votre mot de passe Mimorya"
        );
        assert!(mail.text.starts_with("Bonjour Zoé Leroy,"));
        assert!(
            mail.text
                .contains("compte Mimorya (Néo Maintenance) a été demandée")
        );
        assert!(
            mail.text
                .contains("\nhttps://app.exemple.fr/reset-password#jeton\n")
        );
        assert!(
            mail.text
                .contains("expire le lundi 26 octobre 2026 à 06:42 (heure de Paris)"),
            "{}",
            mail.text
        );
        assert!(mail.text.contains("ignorez ce message"));
    }

    #[test]
    fn names_the_administrator_who_sent_it() {
        let mail = mail(Some("Paul Renaud"));
        assert!(
            mail.text
                .contains("Paul Renaud vous envoie ce lien pour réinitialiser"),
            "{}",
            mail.text
        );
        assert!(
            mail.text
                .contains("En cas de doute, contactez Paul Renaud.")
        );
        assert!(!mail.text.contains("a été demandée"));
    }
}
