//! The processing activities the platform carries out for a company, each
//! with its purpose, the people and data it concerns, who receives them and
//! how long they are kept. The texts are fixed; the recipients and the
//! durations are the company's own.

use super::facts::{ConnectorKind, RegisterFacts};
use super::recipients::{self, Recipient};
use super::retention::{conversations, departed, episodes};

/// Which activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Accounts,
    Assistance,
    Memory,
    Documents,
    Connectors,
    Gateways,
    Audit,
    Usage,
}

impl ActivityKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Accounts => "accounts",
            Self::Assistance => "assistance",
            Self::Memory => "memory",
            Self::Documents => "documents",
            Self::Connectors => "connectors",
            Self::Gateways => "gateways",
            Self::Audit => "audit",
            Self::Usage => "usage",
        }
    }
}

/// A processing activity, as the register states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub kind: ActivityKind,
    pub name: &'static str,
    pub purpose: &'static str,
    pub subjects: Vec<&'static str>,
    pub data: Vec<&'static str>,
    /// What it says of sensitive data (RGPD art. 9), if anything.
    pub sensitive: Option<&'static str>,
    pub recipients: Vec<Recipient>,
    pub retention: Vec<String>,
    /// What else the company should know of it here.
    pub notes: Vec<String>,
}

const COLLABORATORS: &str = "Collaborateurs de l’entreprise titulaires d’un compte";
const CITED: &str = "Personnes citées dans les demandes et dans les données que lit l’agent \
    (clients, contacts, collègues)";
const TYPED: &str = "Les collaborateurs peuvent y écrire des données sensibles (santé, opinions, \
    appartenance syndicale…) : la plateforme ne les y cherche pas ; à encadrer par la charte \
    d’usage de l’entreprise.";

/// The activities of the company, in the register's order: those of a
/// connector or a gateway only when it has one enabled.
pub(super) fn activities(facts: &RegisterFacts) -> Vec<Activity> {
    let mut all = vec![
        accounts(facts),
        assistance(facts),
        memory(facts),
        documents(facts),
    ];
    if !facts.connectors.is_empty() {
        all.push(connectors(facts));
    }
    if !facts.gateways.is_empty() {
        all.push(gateways(facts));
    }
    all.push(audit(facts));
    all.push(usage());
    all
}

fn accounts(facts: &RegisterFacts) -> Activity {
    let mut recipients: Vec<Recipient> = facts
        .identity_providers
        .iter()
        .map(recipients::identity_provider)
        .collect();
    if facts.scim {
        recipients.push(recipients::scim());
    }
    if facts.mails {
        recipients.push(recipients::mail_relay());
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

fn assistance(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(model, "Modèle des conversations"));
    }
    if let Some(model) = &facts.review_model {
        recipients.push(recipients::model(model, "Contrôle des réponses"));
    }
    if facts.jev.requests {
        recipients.push(recipients::jev(
            "Service de décision : choix du modèle et des compétences, sur la demande",
        ));
    }
    Activity {
        kind: ActivityKind::Assistance,
        name: "Assistance par l’agent",
        purpose: "Répondre aux demandes des collaborateurs et agir pour eux, avec leurs droits : \
            conversations, tâches planifiées, déclencheurs, missions, livrables et validations.",
        subjects: vec![COLLABORATORS, CITED],
        data: vec![
            "Messages des conversations et leurs résumés",
            "Demandes, résultats des outils, livrables, actions proposées et décisions de \
             validation",
            "Consignes et horaires des tâches planifiées, des déclencheurs et des missions",
        ],
        sensitive: Some(TYPED),
        recipients,
        retention: vec![conversations(&facts.retention)],
        notes: Vec::new(),
    }
}

fn memory(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(
            model,
            "Modèle des conversations : mémoires utiles à une demande",
        ));
    }
    if let Some(model) = &facts.embeddings {
        recipients.push(recipients::model(
            model,
            "Recherche par le sens (embeddings)",
        ));
    }
    if facts.jev.memories {
        recipients.push(recipients::jev(
            "Service de décision : données sensibles d’une mémoire ou d’une leçon",
        ));
    }
    Activity {
        kind: ActivityKind::Memory,
        name: "Mémoire de l’agent",
        purpose: "Garder ce que l’agent doit se rappeler : mémoires personnelles avec l’accord \
            de la personne, mémoire partagée de l’entreprise et de ses unités, épisodes des runs \
            et leçons proposées.",
        subjects: vec![COLLABORATORS, CITED],
        data: vec![
            "Titre et texte des mémoires, leur vecteur de recherche",
            "Épisodes des runs et leçons proposées à la personne servie",
        ],
        sensitive: Some(TYPED),
        recipients,
        retention: vec![
            episodes(&facts.retention),
            departed(&facts.retention),
            "Autres mémoires : jusqu’à leur suppression, ou leur terme s’ils en ont un".into(),
        ],
        notes: Vec::new(),
    }
}

fn documents(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(
            model,
            "Modèle des conversations : passages lus pour une demande",
        ));
    }
    if let Some(model) = &facts.embeddings {
        recipients.push(recipients::model(
            model,
            "Recherche par le sens (embeddings)",
        ));
    }
    if facts.jev.documents {
        recipients.push(recipients::jev(
            "Service de décision : type et confidentialité, sur le titre et les trois premières \
             pages",
        ));
    }
    Activity {
        kind: ActivityKind::Documents,
        name: "Documents déposés",
        purpose: "Garder les documents que l’entreprise dépose, pour que l’agent y cherche et \
            les cite à qui peut les lire.",
        subjects: vec![
            "Collaborateurs qui les déposent ou les remplacent",
            "Personnes citées dans les documents",
        ],
        data: vec![
            "Fichier tel que déposé, son texte en passages et leurs vecteurs",
            "Qui l’a déposé et qui l’a remplacé en dernier",
            "Type et confidentialité",
        ],
        sensitive: Some(
            "Un document peut en contenir : l’entreprise choisit ce qu’elle dépose et dans \
             quel espace.",
        ),
        recipients,
        retention: vec![
            "Jusqu’à leur suppression par qui peut écrire leur espace ; une nouvelle version \
             remplace l’ancienne"
                .into(),
        ],
        notes: Vec::new(),
    }
}

fn connectors(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(
            model,
            "Modèle des conversations : contenus lus et actions préparées",
        ));
    }
    recipients.extend(facts.connectors.iter().filter_map(recipients::connector));
    if facts.jev.actions {
        recipients.push(recipients::jev(
            "Service de décision : action et ses paramètres, avant qu’elle parte",
        ));
    }
    if facts.jev.external_contents {
        recipients.push(recipients::jev(
            "Service de décision : début des contenus externes rapportés par un outil",
        ));
    }
    let notes = facts
        .connectors
        .iter()
        .filter(|connector| connector.kind == ConnectorKind::Demo)
        .map(|connector| {
            format!(
                "Connecteur « {} » : démonstration, données fictives, rien ne quitte la plateforme",
                connector.name
            )
        })
        .collect();
    Activity {
        kind: ActivityKind::Connectors,
        name: "Connecteurs et comptes reliés",
        purpose: "Lire dans les outils de l’entreprise et, une fois l’action validée, y écrire, \
            pour le collaborateur servi et avec ses droits.",
        subjects: vec![
            COLLABORATORS,
            "Correspondants et personnes présentes dans les outils (e-mails, agendas, fiches)",
        ],
        data: vec![
            "Contenus lus pour une demande : e-mails, rendez-vous, fiches",
            "E-mails envoyés et rendez-vous créés, tels que validés",
            "Comptes reliés : adresse, permissions accordées, jetons scellés ; mot de passe de \
             la messagerie, scellé",
            "Autorisations OAuth des serveurs MCP, jetons scellés",
        ],
        sensitive: None,
        recipients,
        retention: vec![
            "Comptes reliés et autorisations : jusqu’à leur retrait ou l’effacement du compte"
                .into(),
            "Contenus lus : avec la conversation qui les a demandés".into(),
        ],
        notes,
    }
}

fn gateways(facts: &RegisterFacts) -> Activity {
    Activity {
        kind: ActivityKind::Gateways,
        name: "Passerelles de conversation",
        purpose: "Converser avec l’agent et valider ses actions depuis la messagerie de \
            l’entreprise.",
        subjects: vec![COLLABORATORS],
        data: vec![
            "Identifiant de la personne chez le service et sa liaison à son compte",
            "Messages échangés avec l’agent, boutons de validation",
        ],
        sensitive: None,
        recipients: facts
            .gateways
            .iter()
            .copied()
            .map(recipients::gateway)
            .collect(),
        retention: vec![
            "Identités reliées : jusqu’à leur retrait ou l’effacement du compte".into(),
            "Messages : avec la conversation qu’ils tiennent".into(),
        ],
        notes: Vec::new(),
    }
}

fn audit(facts: &RegisterFacts) -> Activity {
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

fn usage() -> Activity {
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
