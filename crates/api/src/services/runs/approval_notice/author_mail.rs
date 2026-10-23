//! The e-mail telling a scheduled task's author that its run awaits their
//! validation (ADR-082), once per pause, in French and plain text: which
//! task, how many actions and of which risks, where to decide and until
//! when. Neither the actions nor their parameters: the message leaves the
//! platform, and they are shown after signing in.

use chrono::{DateTime, Utc};

use super::message::risk_label;
use crate::mail::{Mail, expiry_label};

pub struct AuthorMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub author: &'a str,
    pub email: &'a str,
    /// The task's title, as its author wrote it (on one line).
    pub task: &'a str,
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
        task,
        count,
        risks,
        link,
        expires_at,
    } = notice;
    let labels: Vec<&str> = risks.iter().map(|r| risk_label(r)).collect();
    let (what, risk_word, see, waiting, first) = if *count == 1 {
        (
            "une action".to_owned(),
            "risque",
            "l’action et ses paramètres exacts, puis la valider ou la refuser",
            "Tant que la demande attend",
            "Elle expire",
        )
    } else {
        (
            format!("{count} actions"),
            if labels.len() == 1 {
                "risque"
            } else {
                "risques"
            },
            "les actions et leurs paramètres exacts, puis les valider ou les refuser",
            "Tant que les demandes attendent",
            "Elles expirent",
        )
    };
    let until = match expires_at {
        Some(at) => format!(" {first} le {}.", expiry_label(*at)),
        None => String::new(),
    };
    Mail {
        to: email.to_string(),
        to_name: author.to_string(),
        subject: format!("Votre tâche planifiée « {task} » attend votre validation"),
        text: format!(
            "Bonjour {author},\n\n\
             Votre tâche planifiée « {task} » a lancé l’assistant {product} ({company}), qui \
             demande votre validation pour {what} ({risk_word} : {}).\n\n\
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
        author_mail(&AuthorMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            author: "Nadia Benali",
            email: "nadia.benali@neo-maintenance.example",
            task: "Point du lundi",
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
}
