//! The e-mail telling a manager that requests await them, one per pause of
//! a run (ADR-060), in French and plain text: for whom, how many and of
//! which risks, where to decide and until when. Neither the actions nor
//! their parameters: the message leaves the platform, and they are shown
//! after signing in, where the right to decide is checked.

use chrono::{DateTime, Utc};
use mimorya_core::autonomy::ToolRisk;

use crate::mail::{Mail, expiry_label};

pub struct ApprovalMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub manager: &'a str,
    pub email: &'a str,
    pub requested_for: &'a str,
    /// How many requests await them.
    pub count: usize,
    /// Their risks, once each.
    pub risks: &'a [&'a str],
    /// The page listing the requests to decide.
    pub link: &'a str,
    /// Their deadline: the earliest (those of one step are the same to the
    /// minute).
    pub expires_at: Option<DateTime<Utc>>,
}

/// What an action of `risk` does, as the interface names it; a value no
/// risk bears any more, as stored.
fn risk_label(risk: &str) -> &str {
    match ToolRisk::parse(risk) {
        Some(ToolRisk::Read) => "lecture",
        Some(ToolRisk::Draft) => "brouillon",
        Some(ToolRisk::Write) => "modification",
        Some(ToolRisk::External) => "impact externe",
        Some(ToolRisk::Irreversible) => "irréversible",
        None => risk,
    }
}

pub fn approval_mail(notice: &ApprovalMail<'_>) -> Mail {
    let ApprovalMail {
        product,
        company,
        manager,
        email,
        requested_for,
        count,
        risks,
        link,
        expires_at,
    } = notice;
    let one = *count == 1;
    let labels: Vec<&str> = risks.iter().map(|r| risk_label(r)).collect();
    let (subject, what, risk_word, see, waiting, first, withdraw) = if one {
        (
            format!("Une action de {requested_for} attend votre validation"),
            format!("une action de {requested_for}"),
            "risque",
            "l’action et ses paramètres exacts, puis la valider ou la refuser",
            "Tant que la demande attend",
            "Elle expire",
            "la retirer",
        )
    } else {
        (
            format!("{count} actions de {requested_for} attendent votre validation"),
            format!("{count} actions de {requested_for}"),
            if labels.len() == 1 {
                "risque"
            } else {
                "risques"
            },
            "les actions et leurs paramètres exacts, puis les valider ou les refuser",
            "Tant que les demandes attendent",
            "Elles expirent",
            "les retirer",
        )
    };
    let until = match expires_at {
        Some(at) => format!(" {first} le {}.", expiry_label(*at)),
        None => String::new(),
    };
    Mail {
        to: email.to_string(),
        to_name: manager.to_string(),
        subject,
        text: format!(
            "Bonjour {manager},\n\n\
             L’assistant {product} ({company}) demande votre validation pour {what} \
             ({risk_word} : {}).\n\n\
             Pour voir {see}, connectez-vous et ouvrez la page des validations :\n\
             {link}\n\n\
             {waiting}, l’agent est en pause.{until} {requested_for} peut aussi {withdraw}.\n",
            labels.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn notice(count: usize, risks: &[&str]) -> Mail {
        approval_mail(&ApprovalMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            manager: "Marc Lefèvre",
            email: "marc.lefevre@neo-maintenance.example",
            requested_for: "Nadia Benali",
            count,
            risks,
            link: "https://mimorya.example/approvals",
            expires_at: Some(Utc.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap()),
        })
    }

    #[test]
    fn names_whom_and_where_never_what() {
        let mail = notice(1, &["write"]);
        assert_eq!(mail.to, "marc.lefevre@neo-maintenance.example");
        assert_eq!(mail.to_name, "Marc Lefèvre");
        assert_eq!(
            mail.subject,
            "Une action de Nadia Benali attend votre validation"
        );
        assert!(mail.text.starts_with("Bonjour Marc Lefèvre,"));
        assert!(
            mail.text
                .contains("pour une action de Nadia Benali (risque : modification)")
        );
        assert!(mail.text.contains("\nhttps://mimorya.example/approvals\n"));
        assert!(
            mail.text
                .contains("Elle expire le mercredi 28 octobre 2026 à 09:00 (heure de Paris)")
        );
        assert!(mail.text.contains("Nadia Benali peut aussi la retirer."));
    }

    #[test]
    fn several_requests_of_a_pause_make_one_message() {
        let mail = notice(3, &["write", "irreversible"]);
        assert_eq!(
            mail.subject,
            "3 actions de Nadia Benali attendent votre validation"
        );
        assert!(
            mail.text
                .contains("pour 3 actions de Nadia Benali (risques : modification, irréversible)")
        );
        assert!(
            mail.text
                .contains("Tant que les demandes attendent, l’agent est en pause.")
        );
        assert!(
            mail.text
                .contains("Elles expirent le mercredi 28 octobre 2026")
        );
        assert!(mail.text.contains("peut aussi les retirer."));
        // One risk among several requests.
        assert!(
            notice(2, &["write"])
                .text
                .contains("(risque : modification)")
        );
    }

    #[test]
    fn an_unknown_risk_is_named_as_stored() {
        assert_eq!(risk_label("nouveau"), "nouveau");
    }
}
