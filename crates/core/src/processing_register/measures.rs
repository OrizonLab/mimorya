//! The register's general description of the security measures (RGPD
//! art. 30, 1, g, and 32), some of them the company's own settings, and
//! what the company alone can complete.

use super::facts::RegisterFacts;
use crate::second_factor::Requirement;

pub(super) fn measures(facts: &RegisterFacts) -> Vec<String> {
    let mut all: Vec<String> = vec![
        "Données de chaque entreprise isolées par la sécurité au niveau des lignes de la base"
            .into(),
        "Clés, jetons et mots de passe des services chiffrés (AES-256-GCM), jamais écrits dans \
         les journaux ni dans l’audit"
            .into(),
        "Mots de passe gardés en empreinte (Argon2id) ; débit des tentatives de connexion \
         limité"
            .into(),
        second_factor(facts.second_factor).into(),
    ];
    if !facts.identity_providers.is_empty() {
        all.push("Connexion par le fournisseur d’identité de l’entreprise".into());
    }
    all.extend(
        [
            "Agent soumis aux droits de la personne servie ; ses actions qui modifient \
             quelque chose validées par un humain selon le niveau d’autonomie",
            "Journal d’audit chaîné et scellé, vérifiable par l’entreprise",
            "Échanges chiffrés (TLS) ; en-têtes de sécurité et protection contre la \
             falsification de requêtes",
            "Sauvegardes de la base chiffrées par l’outil de la plateforme ; une donnée \
             supprimée y reste jusqu’à leur retrait, selon la conservation fixée par \
             l’exploitant",
        ]
        .map(String::from),
    );
    all
}

fn second_factor(requirement: Requirement) -> &'static str {
    match requirement {
        Requirement::Optional => {
            "Second facteur (TOTP) offert à chaque compte à mot de passe, exigé de personne"
        }
        Requirement::Administrators => {
            "Second facteur (TOTP) exigé des administrateurs qui se connectent par mot de passe"
        }
        Requirement::Everyone => {
            "Second facteur (TOTP) exigé de chaque compte qui se connecte par mot de passe"
        }
    }
}

/// What only the company, controller of the processing, can say.
pub(super) const TO_COMPLETE: [&str; 7] = [
    "Coordonnées du responsable du traitement, de son représentant et de son délégué à la \
     protection des données",
    "Coordonnées de l’exploitant de la plateforme, sous-traitant qui héberge les données et \
     leurs sauvegardes, et son contrat (article 28)",
    "Base légale de chaque traitement (contrat de travail, intérêt légitime, obligation \
     légale…)",
    "Garanties de chaque transfert hors de l’Union européenne",
    "Information des collaborateurs et des personnes concernées",
    "Analyse d’impact (AIPD) lorsque le traitement l’exige",
    "Conservation des données que l’entreprise exporte (journal d’audit dans son SIEM, \
     exports personnels)",
];
