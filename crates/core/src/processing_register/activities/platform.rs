//! The activities of the platform itself: its accounts, its audit log, its
//! consumption.

use super::{Activity, ActivityKind, COLLABORATORS};
use crate::processing_register::facts::RegisterFacts;
use crate::processing_register::recipients::{self, Recipient};

pub(super) fn accounts(facts: &RegisterFacts) -> Activity {
    let mut recipients: Vec<Recipient> = facts
        .identity_providers
        .iter()
        .map(recipients::identity_provider)
        .collect();
    if facts.scim {
        recipients.push(recipients::scim());
    }
    if let Some(relay) = &facts.mail_relay {
        recipients.push(recipients::mail_relay(
            relay,
            "Invitations, mots de passe oubliés et avis aux administrateurs (fourniture du \
             modèle, propositions de compétences et de politiques)",
        ));
    }
    Activity {
        kind: ActivityKind::Accounts,
        name: "Comptes et connexion",
        purpose: "Gérer les comptes des collaborateurs, leur connexion, leur organisation et \
            leurs droits sur la plateforme.",
        subjects: vec![COLLABORATORS],
        data: vec![
            "Identité : nom, adresse e-mail professionnelle, fonction",
            "Organisation : unités, rôles, habilitations, absences déclarées",
            "Connexion : empreinte du mot de passe (Argon2id), sessions (adresse IP, \
             navigateur), second facteur (secret scellé, codes de secours en empreinte)",
            "Identités chez le fournisseur d’identité et identifiant SCIM",
            "E-mails que chacun choisit de recevoir",
        ],
        sensitive: None,
        recipients,
        retention: vec![
            "Compte : jusqu’à son effacement par un administrateur, une fois désactivé".into(),
            "Sessions : 30 jours après leur fin".into(),
        ],
        notes: Vec::new(),
    }
}

pub(super) fn audit(facts: &RegisterFacts) -> Activity {
    Activity {
        kind: ActivityKind::Audit,
        name: "Journal d’audit",
        purpose: "Tracer les accès et les actions, pour la sécurité et la preuve, sans le \
            contenu des messages.",
        subjects: vec![COLLABORATORS],
        data: vec![
            "Identifiant du compte, action, résultat et date",
            "Adresse IP des connexions",
            "Identifiant chez Slack, Telegram ou Teams des identités reliées",
        ],
        sensitive: None,
        recipients: if facts.audit_export {
            vec![recipients::siem()]
        } else {
            Vec::new()
        },
        retention: vec![
            "5 ans, puis supprimé par ses plus anciennes entrées ; une entrée scellée ne \
             s’expurge pas"
                .into(),
        ],
        notes: Vec::new(),
    }
}

pub(super) fn usage() -> Activity {
    Activity {
        kind: ActivityKind::Usage,
        name: "Consommation",
        purpose: "Mesurer l’usage des modèles et le facturer.",
        subjects: vec![COLLABORATORS],
        data: vec!["Appels de modèle : modèle, volumes, coût, personne servie et run"],
        sensitive: None,
        recipients: Vec::new(),
        retention: vec!["5 ans, puis supprimée".into()],
        notes: Vec::new(),
    }
}
