//! The e-mail telling a manager that requests await them, one per pause of
//! a run (ADR-060), or reminding them the day before they expire (ADR-062),
//! in French and plain text: for whom, how many and of which risks, where to
//! decide and until when; to whom an absent manager named to stand in for
//! them, in whose absence (ADR-071). Neither the actions nor
//! their parameters: the message leaves the platform, and they are shown
//! after signing in, where the right to decide is checked.

use chrono::{DateTime, Utc};

use super::wording::{Phrases, phrases, risk_label};
use crate::mail::{Mail, expiry_label};

pub struct ApprovalMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    /// Whom it is addressed to: the manager, or their stand-in.
    pub manager: &'a str,
    pub email: &'a str,
    /// The absent manager's name, when whom they named is told instead.
    pub absent: Option<&'a str>,
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
    /// A reminder of requests already notified, rather than their notice.
    pub reminder: bool,
}

pub fn approval_mail(notice: &ApprovalMail<'_>) -> Mail {
    let ApprovalMail {
        product,
        company,
        manager,
        email,
        absent,
        requested_for,
        count,
        risks,
        link,
        expires_at,
        reminder,
    } = notice;
    let labels: Vec<&str> = risks.iter().map(|r| risk_label(r)).collect();
    let Phrases {
        risk_word,
        see,
        waiting,
        expires: first,
    } = phrases(*count, labels.len());
    let (subject, what, withdraw) = if *count == 1 {
        (
            format!("Une action de {requested_for} attend votre validation"),
            format!("une action de {requested_for}"),
            "la retirer",
        )
    } else {
        (
            format!("{count} actions de {requested_for} attendent votre validation"),
            format!("{count} actions de {requested_for}"),
            "les retirer",
        )
    };
    let until = match expires_at {
        Some(at) => format!(" {first} le {}.", expiry_label(*at)),
        None => String::new(),
    };
    let (subject, asks) = if *reminder {
        (format!("Rappel : {subject}"), "attend toujours")
    } else {
        (subject, "demande")
    };
    let (subject, assistant) = match absent {
        Some(absent) => (
            format!("{subject} (en l’absence de {absent})"),
            format!("En l’absence de {absent}, l’assistant"),
        ),
        None => (subject, "L’assistant".to_owned()),
    };
    Mail {
        to: email.to_string(),
        to_name: manager.to_string(),
        subject,
        text: format!(
            "Bonjour {manager},\n\n\
             {assistant} {product} ({company}) {asks} votre validation pour {what} \
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
        mail(count, risks, false)
    }

    fn mail(count: usize, risks: &[&str], reminder: bool) -> Mail {
        approval_mail(&ApprovalMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            manager: "Marc Lefèvre",
            email: "marc.lefevre@neo-maintenance.example",
            absent: None,
            requested_for: "Nadia Benali",
            count,
            risks,
            link: "https://mimorya.example/approvals",
            expires_at: Some(Utc.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap()),
            reminder,
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
    fn a_reminder_says_the_requests_still_wait() {
        let one = mail(1, &["write"], true);
        assert_eq!(
            one.subject,
            "Rappel : Une action de Nadia Benali attend votre validation"
        );
        assert!(one.text.contains(
            "L’assistant Mimorya (Néo Maintenance) attend toujours votre validation pour une \
             action de Nadia Benali (risque : modification)."
        ));
        assert!(one.text.contains("Elle expire le mercredi 28 octobre 2026"));
        let several = mail(2, &["write"], true);
        assert_eq!(
            several.subject,
            "Rappel : 2 actions de Nadia Benali attendent votre validation"
        );
    }

    #[test]
    fn a_stand_in_is_told_in_whose_absence() {
        let mail = approval_mail(&ApprovalMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            manager: "Julien Moreau",
            email: "julien.moreau@neo-maintenance.example",
            absent: Some("Marc Lefèvre"),
            requested_for: "Nadia Benali",
            count: 1,
            risks: &["write"],
            link: "https://mimorya.example/approvals",
            expires_at: None,
            reminder: true,
        });
        assert_eq!(mail.to_name, "Julien Moreau");
        assert_eq!(
            mail.subject,
            "Rappel : Une action de Nadia Benali attend votre validation \
             (en l’absence de Marc Lefèvre)"
        );
        assert!(mail.text.starts_with("Bonjour Julien Moreau,"));
        assert!(mail.text.contains(
            "En l’absence de Marc Lefèvre, l’assistant Mimorya (Néo Maintenance) attend \
             toujours votre validation pour une action de Nadia Benali"
        ));
    }

    #[test]
    fn an_unknown_risk_is_named_as_stored() {
        assert_eq!(risk_label("nouveau"), "nouveau");
    }
}
