//! The e-mail telling a collaborator that an administrator declared, or
//! ended, their absence (ADR-078), in French and plain text: who did, who
//! stands in for them and when, where to end it. They did not ask for it:
//! the message lets them know, and object.

use chrono::{DateTime, Utc};

use crate::mail::{Mail, moment_label};

/// What the administrator did.
#[derive(Debug, Clone, Copy)]
pub enum AbsenceChange {
    Declared {
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    },
    /// Ended before its end; `started` unless it had yet to begin.
    Ended { started: bool },
}

pub struct AbsenceMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    /// Whom it is addressed to: the absent collaborator.
    pub name: &'a str,
    pub email: &'a str,
    pub administrator: &'a str,
    pub delegate: &'a str,
    pub change: AbsenceChange,
    /// The page where the collaborator ends it, or finds their requests.
    pub link: &'a str,
}

pub fn absence_mail(notice: &AbsenceMail<'_>) -> Mail {
    let AbsenceMail {
        product,
        company,
        name,
        email,
        administrator,
        delegate,
        change,
        link,
    } = notice;
    let (subject, body) = match change {
        AbsenceChange::Declared { starts_at, ends_at } => (
            format!("{administrator} a déclaré votre absence"),
            format!(
                "{administrator} a déclaré votre absence sur {product} pour {company}, du {} au \
                 {} (heure de Paris). Pendant cette période, {delegate} peut décider à votre \
                 place les demandes de validation qui vous sont confiées, là où son rôle le lui \
                 permet, et reçoit les e-mails qui les annoncent.\n\nVous gardez la main : vous \
                 pouvez toujours décider ces demandes, et mettre fin à cette absence dans \
                 « Validations » :\n{link}",
                moment_label(*starts_at),
                moment_label(*ends_at),
            ),
        ),
        AbsenceChange::Ended { started: true } => (
            format!("{administrator} a mis fin à votre absence"),
            format!(
                "{administrator} a mis fin à votre absence sur {product} pour {company} : \
                 {delegate} ne décide plus à votre place les demandes de validation qui vous \
                 sont confiées. Vous les retrouvez dans « Validations » :\n{link}"
            ),
        ),
        AbsenceChange::Ended { started: false } => (
            format!("{administrator} a annulé votre absence"),
            format!(
                "{administrator} a annulé l’absence prévue pour vous sur {product} pour \
                 {company} : {delegate} ne décidera pas à votre place les demandes de \
                 validation qui vous sont confiées. Vous les retrouvez dans « Validations » :\n\
                 {link}"
            ),
        ),
    };
    Mail {
        to: email.to_string(),
        to_name: name.to_string(),
        subject,
        text: format!(
            "Bonjour {name},\n\n{body}\n\nSi vous n’attendiez pas ce message, prévenez \
             l’administrateur de votre entreprise.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn notice(change: AbsenceChange) -> Mail {
        absence_mail(&AbsenceMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            name: "Marc Lefèvre",
            email: "marc@example.test",
            administrator: "Paul Martin",
            delegate: "Julien Moreau",
            change,
            link: "https://app.example.test/approvals",
        })
    }

    #[test]
    fn says_who_declared_it_who_stands_in_and_until_when() {
        let starts_at = Utc.with_ymd_and_hms(2026, 11, 2, 7, 0, 0).unwrap();
        let ends_at = Utc.with_ymd_and_hms(2026, 11, 9, 23, 0, 0).unwrap();
        let mail = notice(AbsenceChange::Declared { starts_at, ends_at });
        assert_eq!(mail.to, "marc@example.test");
        assert_eq!(mail.subject, "Paul Martin a déclaré votre absence");
        assert!(mail.text.starts_with("Bonjour Marc Lefèvre,"));
        assert!(mail.text.contains(
            "du lundi 2 novembre 2026 à 08:00 au mardi 10 novembre 2026 à 00:00 (heure de Paris)"
        ));
        assert!(
            mail.text
                .contains("Julien Moreau peut décider à votre place")
        );
        assert!(mail.text.contains("mettre fin à cette absence"));
        assert!(mail.text.contains("\nhttps://app.example.test/approvals\n"));
        assert!(mail.text.contains("prévenez l’administrateur"));
    }

    #[test]
    fn says_who_ended_it_or_called_it_off() {
        let mail = notice(AbsenceChange::Ended { started: true });
        assert_eq!(mail.subject, "Paul Martin a mis fin à votre absence");
        assert!(
            mail.text
                .contains("Julien Moreau ne décide plus à votre place")
        );
        assert!(!mail.text.contains("heure de Paris"));

        let mail = notice(AbsenceChange::Ended { started: false });
        assert_eq!(mail.subject, "Paul Martin a annulé votre absence");
        assert!(
            mail.text
                .contains("Julien Moreau ne décidera pas à votre place")
        );
        assert!(mail.text.contains("\nhttps://app.example.test/approvals\n"));
    }
}
