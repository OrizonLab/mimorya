//! The e-mail telling an administrator of the access policies that allow
//! policies proposed by another await their decision (ADR-170), in French
//! and plain text: how many, who proposed them, what an approval does, and
//! where to decide. Neither the policies' descriptions nor their conditions:
//! the message leaves the platform, and they are shown after signing in.

use crate::mail::Mail;

pub struct PolicyProposalsMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub administrator: &'a str,
    pub email: &'a str,
    /// Who proposed them, each once, in order.
    pub authors: &'a [String],
    /// How many proposals never announced to this administrator.
    pub count: usize,
    /// The administration.
    pub link: &'a str,
}

pub fn policy_proposals_mail(notice: &PolicyProposalsMail<'_>) -> Mail {
    let PolicyProposalsMail {
        product,
        company,
        administrator,
        email,
        authors,
        count,
        link,
    } = notice;
    let authors = authors.join(", ");
    let (subject, what) = if *count == 1 {
        (
            "Une autorisation proposée attend votre décision".to_string(),
            format!("{authors} propose une politique d’autorisation dans {product} ({company})."),
        )
    } else {
        (
            format!("{count} autorisations proposées attendent votre décision"),
            format!(
                "{count} politiques d’autorisation sont proposées dans {product} ({company}), \
                 par {authors}."
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
             Une autorisation ouvre des données au-delà des rôles : elle ne s’applique qu’une \
             fois approuvée par un autre administrateur des politiques que son auteur, dans les \
             sept jours. Pour la relire, l’approuver ou la refuser, connectez-vous et ouvrez \
             l’onglet « Politiques d’accès » de l’administration :\n\
             {link}\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(authors: &[&str], count: usize) -> Mail {
        let authors: Vec<String> = authors.iter().map(|s| s.to_string()).collect();
        policy_proposals_mail(&PolicyProposalsMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            administrator: "Claire Dumas",
            email: "claire.dumas@neo-maintenance.example",
            authors: &authors,
            count,
            link: "https://mimorya.example/admin",
        })
    }

    #[test]
    fn one_proposal_names_its_author_and_where_to_decide() {
        let one = mail(&["Paul Renaud"], 1);
        assert_eq!(one.to, "claire.dumas@neo-maintenance.example");
        assert_eq!(
            one.subject,
            "Une autorisation proposée attend votre décision"
        );
        assert!(one.text.starts_with("Bonjour Claire Dumas,\n\n"));
        assert!(one.text.contains(
            "Paul Renaud propose une politique d’autorisation dans Mimorya (Néo Maintenance)."
        ));
        assert!(one.text.contains(
            "ouvrez l’onglet « Politiques d’accès » de l’administration :\n\
             https://mimorya.example/admin\n"
        ));
    }

    #[test]
    fn several_proposals_are_counted_with_their_authors() {
        let several = mail(&["Paul Renaud", "Marc Lefèvre"], 3);
        assert_eq!(
            several.subject,
            "3 autorisations proposées attendent votre décision"
        );
        assert!(several.text.contains(
            "3 politiques d’autorisation sont proposées dans Mimorya (Néo Maintenance), par \
             Paul Renaud, Marc Lefèvre."
        ));
    }
}
