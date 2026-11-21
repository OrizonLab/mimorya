//! The activities that reach out of the platform: the company's connectors
//! and the accounts its people link, its conversation gateways.

use super::{Activity, ActivityKind, COLLABORATORS};
use crate::processing_register::facts::{ConnectorKind, RegisterFacts};
use crate::processing_register::recipients;

pub(super) fn connectors(facts: &RegisterFacts) -> Activity {
    let mut recipients = Vec::new();
    if let Some(model) = &facts.runs_model {
        recipients.push(recipients::model(
            model,
            "Modèle des conversations : contenus lus et actions préparées",
        ));
    }
    recipients.extend(facts.connectors.iter().filter_map(recipients::connector));
    if facts.jev && facts.decisions.actions {
        recipients.push(recipients::jev(
            "Service de décision : action et ses paramètres, avant qu’elle parte",
        ));
    }
    if facts.jev && facts.decisions.external_contents {
        recipients.push(recipients::jev(
            "Service de décision : début des contenus rapportés par un outil, pour y chercher \
             des instructions",
        ));
    }
    let notes = facts
        .connectors
        .iter()
        .filter_map(|connector| match connector.kind {
            ConnectorKind::Demo => Some(format!(
                "Connecteur « {} » : démonstration, données fictives ; il n’atteint aucun \
                 service extérieur",
                connector.name
            )),
            ConnectorKind::McpProcess => Some(format!(
                "Connecteur « {} » : programme installé par l’exploitant, lancé par la \
                 plateforme ; les services qu’il joint ne sont pas connus d’elle",
                connector.name
            )),
            _ => None,
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
        sensitive: Some(
            "Les e-mails, rendez-vous et fiches lus peuvent en contenir : la plateforme ne les y \
             cherche pas ; à encadrer par la charte d’usage de l’entreprise.",
        ),
        recipients,
        retention: vec![
            "Comptes reliés à Microsoft 365 ou Google : jusqu’à leur retrait, la suppression \
             du connecteur ou la désactivation du compte"
                .into(),
            "Comptes de messagerie : jusqu’à leur retrait, le changement d’hôte d’un serveur, \
             la suppression du connecteur ou la désactivation du compte"
                .into(),
            "Autorisations de chacun des serveurs MCP : jusqu’à leur retrait, un \
             renouvellement refusé, un changement de l’adresse ou de la déclaration du \
             serveur ou de qui l’autorise, la suppression du connecteur ou la désactivation \
             du compte"
                .into(),
            "Autorisations de l’entreprise : jusqu’à leur retrait ou leur remplacement, un \
             changement de l’adresse ou de la déclaration du serveur ou de qui l’autorise, \
             ou la suppression du connecteur"
                .into(),
            "Contenus lus : avec la conversation qui les a demandés".into(),
        ],
        notes,
    }
}

pub(super) fn gateways(facts: &RegisterFacts) -> Activity {
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
            "Identités reliées : jusqu’à leur retrait, celui de la passerelle ou la \
             désactivation du compte"
                .into(),
            "Messages : avec la conversation qu’ils tiennent".into(),
        ],
        notes: Vec::new(),
    }
}
