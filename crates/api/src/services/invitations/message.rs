//! The invitation e-mail, in French and plain text: who invites, to which
//! company, the link and until when it works.

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use mimorya_agent::runtime::french_date;

use crate::mail::Mail;

pub struct InvitationMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub inviter: &'a str,
    pub invited_name: &'a str,
    pub invited_email: &'a str,
    pub link: &'a str,
    pub expires_at: DateTime<Utc>,
}

/// When the link stops working, as the invited person reads it.
pub fn expiry_label(expires_at: DateTime<Utc>) -> String {
    format!(
        "{} à {} (heure de Paris)",
        french_date(expires_at, Paris),
        expires_at.with_timezone(&Paris).format("%H:%M")
    )
}

pub fn invitation_mail(invitation: &InvitationMail<'_>) -> Mail {
    let InvitationMail {
        product,
        company,
        inviter,
        invited_name,
        invited_email,
        link,
        expires_at,
    } = invitation;
    Mail {
        to: invited_email.to_string(),
        to_name: invited_name.to_string(),
        subject: format!("{company} vous invite sur {product}"),
        text: format!(
            "Bonjour {invited_name},\n\n\
             {inviter} vous a créé un compte {product} pour {company}.\n\n\
             Pour choisir votre mot de passe et activer votre compte, ouvrez ce lien :\n\
             {link}\n\n\
             Ce lien est personnel et ne sert qu’une fois. Il expire le {}.\n\n\
             Si vous n’attendiez pas cette invitation, ignorez ce message : le compte ne sera \
             pas activé.\n",
            expiry_label(*expires_at)
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn says_who_invites_where_and_until_when() {
        let mail = invitation_mail(&InvitationMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            inviter: "Paul Martin",
            invited_name: "Zoé Leroy",
            invited_email: "zoe.leroy@neo-maintenance.example",
            link: "https://app.exemple.fr/invitation#jeton",
            expires_at: Utc.with_ymd_and_hms(2026, 10, 26, 5, 42, 0).unwrap(),
        });
        assert_eq!(mail.to, "zoe.leroy@neo-maintenance.example");
        assert_eq!(mail.subject, "Néo Maintenance vous invite sur Mimorya");
        assert!(mail.text.starts_with("Bonjour Zoé Leroy,"));
        assert!(
            mail.text
                .contains("Paul Martin vous a créé un compte Mimorya")
        );
        assert!(
            mail.text
                .contains("\nhttps://app.exemple.fr/invitation#jeton\n")
        );
        assert!(
            mail.text
                .contains("expire le lundi 26 octobre 2026 à 06:42 (heure de Paris)"),
            "{}",
            mail.text
        );
    }
}
