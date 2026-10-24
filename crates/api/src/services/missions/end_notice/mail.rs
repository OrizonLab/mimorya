//! The e-mail telling a mission's author that it is over (ADR-091), in
//! French and plain text: which mission, whether it is done and how its
//! final answer was reviewed, or that it stopped unfinished, and where to
//! read it. Neither its answer nor why it stopped: the message leaves the
//! platform, and they are shown after signing in.

use crate::mail::Mail;

/// How the review of a mission's final answer turned out (ADR-090).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    /// A reviewer judged it against every criterion.
    Verified,
    /// Points remain unverified.
    Unverified,
    /// No reviewer could judge it.
    Unjudged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Done(Review),
    /// Past its limits, or its last stage failed.
    Unfinished,
}

impl End {
    /// The mission's status as told, audited with the notice.
    pub fn status(self) -> &'static str {
        match self {
            Self::Done(_) => "done",
            Self::Unfinished => "failed",
        }
    }
}

pub struct EndMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub owner: &'a str,
    pub email: &'a str,
    /// The mission's title, as its author wrote it (on one line).
    pub title: &'a str,
    pub end: End,
    /// The « Missions » page.
    pub link: &'a str,
}

pub fn end_mail(notice: &EndMail<'_>) -> Mail {
    let EndMail {
        product,
        company,
        owner,
        email,
        title,
        end,
        link,
    } = notice;
    let (subject, what, read) = match end {
        End::Done(review) => {
            let review = match review {
                Review::Verified => {
                    "Sa réponse finale a été vérifiée au regard de ses critères de réussite."
                }
                Review::Unverified => {
                    "Des points de sa réponse finale restent non vérifiés au regard de ses \
                     critères de réussite."
                }
                Review::Unjudged => {
                    "Sa réponse finale n’a pas pu être relue au regard de ses critères de \
                     réussite."
                }
            };
            (
                format!("Votre mission « {title} » est terminée"),
                format!(
                    "L’assistant {product} ({company}) a terminé votre mission « {title} ». {review}"
                ),
                "Pour lire sa réponse",
            )
        }
        End::Unfinished => (
            format!("Votre mission « {title} » s’est arrêtée inachevée"),
            format!(
                "Votre mission « {title} », que l’assistant {product} ({company}) menait en \
                 arrière-plan, s’est arrêtée inachevée."
            ),
            "Pour savoir pourquoi, et où elle en est restée",
        ),
    };
    Mail {
        to: email.to_string(),
        to_name: owner.to_string(),
        subject,
        text: format!(
            "Bonjour {owner},\n\n\
             {what}\n\n\
             {read}, connectez-vous et ouvrez « Missions » :\n\
             {link}\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(end: End) -> Mail {
        end_mail(&EndMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            owner: "Marc Lefèvre",
            email: "marc.lefevre@neo-maintenance.example",
            title: "Suivi de la semaine",
            end,
            link: "https://mimorya.example/missions",
        })
    }

    #[test]
    fn a_mission_done_says_how_its_answer_was_reviewed() {
        let verified = mail(End::Done(Review::Verified));
        assert_eq!(verified.to, "marc.lefevre@neo-maintenance.example");
        assert_eq!(
            verified.subject,
            "Votre mission « Suivi de la semaine » est terminée"
        );
        assert!(verified.text.starts_with("Bonjour Marc Lefèvre,\n\n"));
        assert!(verified.text.contains(
            "L’assistant Mimorya (Néo Maintenance) a terminé votre mission « Suivi de la \
             semaine ». Sa réponse finale a été vérifiée au regard de ses critères de réussite."
        ));
        assert!(verified.text.contains(
            "Pour lire sa réponse, connectez-vous et ouvrez « Missions » :\n\
             https://mimorya.example/missions\n"
        ));
        let unverified = mail(End::Done(Review::Unverified));
        assert!(unverified.text.contains("restent non vérifiés"));
        let unjudged = mail(End::Done(Review::Unjudged));
        assert!(unjudged.text.contains("n’a pas pu être relue"));
        assert!(!unjudged.text.contains("a été vérifiée"));
    }

    #[test]
    fn a_mission_unfinished_says_so_and_where_to_see_why() {
        let unfinished = mail(End::Unfinished);
        assert_eq!(
            unfinished.subject,
            "Votre mission « Suivi de la semaine » s’est arrêtée inachevée"
        );
        assert!(unfinished.text.contains("s’est arrêtée inachevée."));
        assert!(unfinished.text.contains(
            "Pour savoir pourquoi, et où elle en est restée, connectez-vous et ouvrez « Missions »"
        ));
    }
}
