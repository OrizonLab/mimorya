//! The e-mail telling a collaborator that an administrator declared, or
//! ended, their absence (ADR-078), in French and plain text: who did, who
//! stands in for them and when, where to end it. They did not ask for it:
//! the message lets them know, and object.

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use mimorya_agent::runtime::french_date;

use crate::mail::Mail;

pub struct AbsenceMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    /// Whom it is addressed to: the absent collaborator.
    pub name: &'a str,
    pub email: &'a str,
    pub administrator: &'a str,
    pub delegate: &'a str,
    /// From when until when, as declared; `None` once ended.
    pub period: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// The page where the collaborator ends it, or finds their requests.
    pub link: &'a str,
}

/// A moment as the collaborator reads it, in Paris time.
fn moment(at: DateTime<Utc>) -> String {
    format!(
        "{} à {}",
        french_date(at, Paris),
        at.with_timezone(&Paris).format("%H:%M")
    )
}

pub fn absence_mail(notice: &AbsenceMail<'_>) -> Mail {
    let AbsenceMail {
        product,
        company,
        name,
        email,
        administrator,
        delegate,
        period,
        link,
    } = notice;
    let (subject, body) = match period {
        Some((starts_at, ends_at)) => (
            format!("{administrator} a déclaré votre absence"),
            format!(
                "{administrator} a déclaré votre absence sur {product} pour {company}, du {} au \
                 {} (heure de Paris). Pendant cette période, {delegate} peut décider à votre \
                 place les demandes de validation qui vous sont confiées, là où son rôle le lui \
                 permet, et reçoit les e-mails qui les annoncent.\n\nVous gardez la main : vous \
                 pouvez toujours décider ces demandes, et mettre fin à cette absence dans \
                 « Validations » :\n{link}",
                moment(*starts_at),
                moment(*ends_at),
            ),
        ),
        None => (
            format!("{administrator} a mis fin à votre absence"),
            format!(
                "{administrator} a mis fin à votre absence sur {product} pour {company} : \
                 {delegate} ne décide plus à votre place les demandes de validation qui vous \
                 sont confiées. Vous les retrouvez dans « Validations » :\n{link}"
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

    fn notice(period: Option<(DateTime<Utc>, DateTime<Utc>)>) -> Mail {
        absence_mail(&AbsenceMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            name: "Marc Lefèvre",
            email: "marc@example.test",
            administrator: "Paul Martin",
            delegate: "Julien Moreau",
            period,
            link: "https://app.example.test/approvals",
        })
    }

    #[test]
    fn says_who_declared_it_who_stands_in_and_until_when() {
        let starts = Utc.with_ymd_and_hms(2026, 11, 2, 7, 0, 0).unwrap();
        let ends = Utc.with_ymd_and_hms(2026, 11, 9, 23, 0, 0).unwrap();
        let mail = notice(Some((starts, ends)));
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
    fn says_who_ended_it() {
        let mail = notice(None);
        assert_eq!(mail.subject, "Paul Martin a mis fin à votre absence");
        assert!(
            mail.text
                .contains("Julien Moreau ne décide plus à votre place")
        );
        assert!(!mail.text.contains("heure de Paris"));
    }
}
