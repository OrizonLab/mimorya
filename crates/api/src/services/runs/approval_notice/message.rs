//! The e-mail telling a manager that a request awaits them, in French and
//! plain text: for whom, which kind of action, where to decide and until
//! when. Neither the action nor its parameters: the message leaves the
//! platform, and they are shown after signing in, where the right to decide
//! is checked (ADR-060).

use chrono::{DateTime, Utc};

use crate::mail::{Mail, expiry_label};

pub struct ApprovalMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub manager: &'a str,
    pub email: &'a str,
    pub requested_for: &'a str,
    pub risk: &'a str,
    /// The page listing the requests to decide.
    pub link: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

/// What an action of `risk` does, as the interface names it.
fn risk_label(risk: &str) -> &str {
    match risk {
        "read" => "lecture",
        "draft" => "brouillon",
        "write" => "modification",
        "external" => "impact externe",
        "irreversible" => "irréversible",
        other => other,
    }
}

pub fn approval_mail(notice: &ApprovalMail<'_>) -> Mail {
    let ApprovalMail {
        product,
        company,
        manager,
        email,
        requested_for,
        risk,
        link,
        expires_at,
    } = notice;
    let until = match expires_at {
        Some(at) => format!(" Elle expire le {}.", expiry_label(*at)),
        None => String::new(),
    };
    Mail {
        to: email.to_string(),
        to_name: manager.to_string(),
        subject: format!("Une action de {requested_for} attend votre validation"),
        text: format!(
            "Bonjour {manager},\n\n\
             L’assistant {product} ({company}) demande votre validation pour une action de \
             {requested_for} (risque : {}).\n\n\
             Pour voir l’action et ses paramètres exacts, puis la valider ou la refuser, \
             connectez-vous et ouvrez la page des validations :\n\
             {link}\n\n\
             Tant que la demande attend, l’agent est en pause.{until} {requested_for} peut \
             aussi la retirer.\n",
            risk_label(risk)
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn names_whom_and_where_never_what() {
        let mail = approval_mail(&ApprovalMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            manager: "Marc Lefèvre",
            email: "marc.lefevre@neo-maintenance.example",
            requested_for: "Nadia Benali",
            risk: "write",
            link: "https://mimorya.example/approvals",
            expires_at: Some(Utc.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap()),
        });
        assert_eq!(mail.to, "marc.lefevre@neo-maintenance.example");
        assert_eq!(mail.to_name, "Marc Lefèvre");
        assert_eq!(
            mail.subject,
            "Une action de Nadia Benali attend votre validation"
        );
        assert!(mail.text.starts_with("Bonjour Marc Lefèvre,"));
        assert!(mail.text.contains("(risque : modification)"));
        assert!(mail.text.contains("\nhttps://mimorya.example/approvals\n"));
        assert!(
            mail.text
                .contains("mercredi 28 octobre 2026 à 09:00 (heure de Paris)")
        );
    }

    #[test]
    fn an_unknown_risk_is_named_as_stored() {
        assert_eq!(risk_label("nouveau"), "nouveau");
    }
}
