//! The e-mail telling a scheduled task's author that its run awaits their
//! validation (ADR-082), or a mission's author that a stage of it does
//! (ADR-091), once per pause, in French and plain text: which task or
//! mission, how many actions and of which risks, where to decide and until
//! when. Neither the actions nor their parameters: the message leaves the
//! platform, and they are shown after signing in.

use chrono::{DateTime, Utc};

use super::wording::{Phrases, phrases, risk_label};
use crate::mail::{Mail, expiry_label};

pub struct AuthorMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub author: &'a str,
    pub email: &'a str,
    /// The title of the task, or of the mission, as its author wrote it
    /// (on one line).
    pub title: &'a str,
    /// Whether the run is a stage of a mission, not a scheduled task's.
    pub of_mission: bool,
    /// How many requests await them.
    pub count: usize,
    /// Their risks, once each.
    pub risks: &'a [&'a str],
    /// The run's conversation, where they are decided.
    pub link: &'a str,
    /// Their deadline: the earliest.
    pub expires_at: Option<DateTime<Utc>>,
}

pub fn author_mail(notice: &AuthorMail<'_>) -> Mail {
    let AuthorMail {
        product,
        company,
        author,
        email,
        title,
        of_mission,
        count,
        risks,
        link,
        expires_at,
    } = notice;
    let labels: Vec<&str> = risks.iter().map(|r| risk_label(r)).collect();
    let Phrases {
        risk_word,
        see,
        waiting,
        expires: first,
    } = phrases(*count, labels.len());
    let what = if *count == 1 {
        "une action".to_owned()
    } else {
        format!("{count} actions")
    };
    let until = match expires_at {
        Some(at) => format!(" {first} le {}.", expiry_label(*at)),
        None => String::new(),
    };
    let (subject, origin) = if *of_mission {
        (
            format!("Votre mission « {title} » attend votre validation"),
            format!(
                "Votre mission « {title} », que l’assistant {product} ({company}) mène en \
                 arrière-plan, demande"
            ),
        )
    } else {
        (
            format!("Votre tâche planifiée « {title} » attend votre validation"),
            format!(
                "Votre tâche planifiée « {title} » a lancé l’assistant {product} ({company}), \
                 qui demande"
            ),
        )
    };
    Mail {
        to: email.to_string(),
        to_name: author.to_string(),
        subject,
        text: format!(
            "Bonjour {author},\n\n\
             {origin} votre validation pour {what} ({risk_word} : {}).\n\n\
             Pour voir {see}, connectez-vous et ouvrez sa conversation :\n\
             {link}\n\n\
             {waiting}, l’agent est en pause.{until}\n",
            labels.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn mail(count: usize, risks: &[&str]) -> Mail {
        mail_of(false, count, risks)
    }

    fn mail_of(of_mission: bool, count: usize, risks: &[&str]) -> Mail {
        author_mail(&AuthorMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            author: "Nadia Benali",
            email: "nadia.benali@neo-maintenance.example",
            title: "Point du lundi",
            of_mission,
            count,
            risks,
            link: "https://mimorya.example/chat/42",
            expires_at: Some(Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap()),
        })
    }

    #[test]
    fn names_the_task_and_where_to_decide() {
        let one = mail(1, &["write"]);
        assert_eq!(one.to, "nadia.benali@neo-maintenance.example");
        assert_eq!(
            one.subject,
            "Votre tâche planifiée « Point du lundi » attend votre validation"
        );
        assert!(one.text.starts_with("Bonjour Nadia Benali,\n\n"));
        assert!(
            one.text
                .contains("demande votre validation pour une action (risque : modification)")
        );
        assert!(
            one.text
                .contains("ouvrez sa conversation :\nhttps://mimorya.example/chat/42")
        );
        assert!(
            one.text
                .contains("Tant que la demande attend, l’agent est en pause. Elle expire le ")
        );
    }

    #[test]
    fn counts_several_actions_and_their_risks() {
        let several = mail(3, &["write", "external"]);
        assert!(
            several
                .text
                .contains("pour 3 actions (risques : modification, impact externe)")
        );
        assert!(several.text.contains("Tant que les demandes attendent"));
        assert!(several.text.contains("Elles expirent le "));
    }

    #[test]
    fn names_the_mission_a_stage_of_which_waits() {
        let stage = mail_of(true, 1, &["write"]);
        assert_eq!(
            stage.subject,
            "Votre mission « Point du lundi » attend votre validation"
        );
        assert!(stage.text.contains(
            "Votre mission « Point du lundi », que l’assistant Mimorya (Néo Maintenance) mène \
             en arrière-plan, demande votre validation pour une action (risque : modification)."
        ));
        assert!(
            stage
                .text
                .contains("ouvrez sa conversation :\nhttps://mimorya.example/chat/42")
        );
    }
}
