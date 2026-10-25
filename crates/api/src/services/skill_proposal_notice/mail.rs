//! The e-mail telling a skills' administrator that new proposals of the
//! agent's await their decision (ADR-102), in French and plain text: how
//! many, for which skills, that each text was validated before it left its
//! conversation, and where to decide. Neither the procedure nor whom it
//! was proposed for, nor who validated it: the message leaves the
//! platform, and they are shown after signing in.

use crate::mail::Mail;

pub struct ProposalsMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub administrator: &'a str,
    pub email: &'a str,
    /// The skills proposals were made for, each once, in order.
    pub skills: &'a [String],
    /// How many proposals never announced, a skill having several.
    pub count: usize,
    /// The administration.
    pub link: &'a str,
}

pub fn proposals_mail(notice: &ProposalsMail<'_>) -> Mail {
    let ProposalsMail {
        product,
        company,
        administrator,
        email,
        skills,
        count,
        link,
    } = notice;
    let named: Vec<String> = skills.iter().map(|s| format!("« {s} »")).collect();
    let named = named.join(", ");
    let (subject, what) = if *count == 1 {
        (
            "Une nouvelle proposition de compétence attend votre décision".to_string(),
            format!(
                "L’assistant {product} ({company}) propose une compétence, dont le texte a été \
                 validé avant son envoi : {named}."
            ),
        )
    } else {
        (
            format!("{count} nouvelles propositions de compétences attendent votre décision"),
            format!(
                "L’assistant {product} ({company}) propose {count} compétences ou nouvelles \
                 versions, dont chaque texte a été validé avant son envoi : {named}."
            ),
        )
    };
    Mail {
        to: email.to_string(),
        to_name: administrator.to_string(),
        subject,
        text: format!(
            "Bonjour {administrator},\n\n\
             {what}\n\n\
             Aucune n’est utilisée avant que vous l’activiez. Pour les relire, les activer ou \
             les rejeter, connectez-vous et ouvrez l’onglet « Compétences » de \
             l’administration :\n\
             {link}\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(skills: &[&str], count: usize) -> Mail {
        let skills: Vec<String> = skills.iter().map(|s| s.to_string()).collect();
        proposals_mail(&ProposalsMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            administrator: "Paul Renaud",
            email: "paul.renaud@neo-maintenance.example",
            skills: &skills,
            count,
            link: "https://mimorya.example/admin",
        })
    }

    #[test]
    fn one_proposal_names_its_skill_and_where_to_decide() {
        let one = mail(&["relance-fournisseur"], 1);
        assert_eq!(one.to, "paul.renaud@neo-maintenance.example");
        assert_eq!(
            one.subject,
            "Une nouvelle proposition de compétence attend votre décision"
        );
        assert!(one.text.starts_with("Bonjour Paul Renaud,\n\n"));
        assert!(one.text.contains(
            "L’assistant Mimorya (Néo Maintenance) propose une compétence, dont le texte a été \
             validé avant son envoi : « relance-fournisseur »."
        ));
        assert!(one.text.contains(
            "ouvrez l’onglet « Compétences » de l’administration :\n\
             https://mimorya.example/admin\n"
        ));
    }

    #[test]
    fn several_proposals_are_counted_and_their_skills_named_once() {
        let several = mail(&["relance-fournisseur", "synthese-hebdo"], 3);
        assert_eq!(
            several.subject,
            "3 nouvelles propositions de compétences attendent votre décision"
        );
        assert!(several.text.contains(
            "propose 3 compétences ou nouvelles versions, dont chaque texte a été validé avant \
             son envoi : « relance-fournisseur », « synthese-hebdo »."
        ));
    }
}
