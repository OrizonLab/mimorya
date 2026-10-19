//! The reset e-mail, in French and plain text: which account, the link and
//! until when it works, and what to do if the request was not yours.

use chrono::{DateTime, Utc};

use crate::mail::{Mail, expiry_label};

pub struct ResetMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub name: &'a str,
    pub email: &'a str,
    pub link: &'a str,
    pub expires_at: DateTime<Utc>,
}

pub fn reset_mail(reset: &ResetMail<'_>) -> Mail {
    let ResetMail {
        product,
        company,
        name,
        email,
        link,
        expires_at,
    } = reset;
    Mail {
        to: email.to_string(),
        to_name: name.to_string(),
        subject: format!("Réinitialisation de votre mot de passe {product}"),
        text: format!(
            "Bonjour {name},\n\n\
             La réinitialisation du mot de passe de votre compte {product} ({company}) a été \
             demandée.\n\n\
             Pour choisir un nouveau mot de passe, ouvrez ce lien :\n\
             {link}\n\n\
             Ce lien est personnel et ne sert qu’une fois. Il expire le {}. Une fois le \
             nouveau mot de passe choisi, toutes vos autres sessions seront fermées.\n\n\
             Si vous n’êtes pas à l’origine de cette demande, ignorez ce message : votre mot \
             de passe reste inchangé.\n",
            expiry_label(*expires_at)
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn names_the_account_the_link_and_its_expiry() {
        let mail = reset_mail(&ResetMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            name: "Zoé Leroy",
            email: "zoe.leroy@neo-maintenance.example",
            link: "https://app.exemple.fr/reset-password#jeton",
            expires_at: Utc.with_ymd_and_hms(2026, 10, 26, 5, 42, 0).unwrap(),
        });
        assert_eq!(mail.to, "zoe.leroy@neo-maintenance.example");
        assert_eq!(mail.to_name, "Zoé Leroy");
        assert_eq!(
            mail.subject,
            "Réinitialisation de votre mot de passe Mimorya"
        );
        assert!(mail.text.starts_with("Bonjour Zoé Leroy,"));
        assert!(mail.text.contains("compte Mimorya (Néo Maintenance)"));
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
    }
}
