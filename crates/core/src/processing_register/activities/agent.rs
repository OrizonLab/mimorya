//! The activities of the agent: its assistance, its memory, the documents
//! it searches. Their model receives, besides, what the company's decision
//! service asks it (ADR-213): it decides each usage set, after Jev when the
//! company chose Jev.

use super::{Activity, ActivityKind, CITED, COLLABORATORS, TYPED};
use crate::processing_register::facts::RegisterFacts;
use crate::processing_register::recipients;
use crate::processing_register::retention::{conversations, departed, episodes};

/// The memories' sensitive data, the decision service asked about them.
const GUARDED: &str = "Les collaborateurs peuvent y écrire des données sensibles (santé, \
    opinions, appartenance syndicale…) : le service de décision les cherche dans chaque mémoire \
    personnelle ou leçon avant qu’elle soit enregistrée ; à encadrer par la charte d’usage de \
    l’entreprise.";

pub(super) fn assistance(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(model, "Modèle des conversations"));
    }
    if let Some(model) = &facts.review_model {
        recipients.push(recipients::model(model, "Contrôle des réponses"));
    }
    if facts.jev && facts.decisions.requests {
        recipients.push(recipients::jev(
            "Service de décision : choix du modèle et des compétences, sur la demande",
        ));
    }
    if let Some(relay) = &facts.mail_relay {
        recipients.push(recipients::mail_relay(
            relay,
            "Avis par e-mail : missions terminées, validations à donner, absences à remplacer",
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
        retention: vec![
            conversations(&facts.retention),
            "Missions : avec leur conversation".into(),
            "Tâches planifiées et déclencheurs : jusqu’à leur suppression par leur auteur ou \
             l’effacement de son compte"
                .into(),
        ],
        notes: Vec::new(),
    }
}

pub(super) fn memory(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        let role = if facts.decisions.memories {
            "Modèle des conversations : mémoires utiles à une demande ; titre et texte de chaque \
             mémoire ou leçon à enregistrer, pour y chercher des données sensibles"
        } else {
            "Modèle des conversations : mémoires utiles à une demande"
        };
        recipients.push(recipients::model(model, role));
    }
    if let Some(model) = &facts.embeddings {
        recipients.push(recipients::model(
            model,
            "Recherche par le sens (embeddings)",
        ));
    }
    if facts.jev && facts.decisions.memories {
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
        sensitive: Some(if facts.decisions.memories {
            GUARDED
        } else {
            TYPED
        }),
        recipients,
        retention: vec![
            episodes(&facts.retention),
            departed(&facts.retention),
            "Autres mémoires : jusqu’à leur suppression, ou leur terme si elles en ont un".into(),
        ],
        notes: Vec::new(),
    }
}

pub(super) fn documents(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        let role = if facts.decisions.documents {
            "Modèle des conversations : passages lus pour une demande ; titre et trois premières \
             pages de chaque document déposé, pour son classement"
        } else {
            "Modèle des conversations : passages lus pour une demande"
        };
        recipients.push(recipients::model(model, role));
    }
    if let Some(model) = &facts.embeddings {
        recipients.push(recipients::model(
            model,
            "Recherche par le sens (embeddings)",
        ));
    }
    if facts.jev && facts.decisions.documents {
        recipients.push(recipients::jev(
            "Service de décision : type et confidentialité, sur le titre et les trois premières \
             pages",
        ));
    }
    if facts.jev && facts.decisions.external_contents {
        recipients.push(recipients::jev(
            "Service de décision : début des passages lus pour une demande, pour y chercher des \
             instructions",
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
