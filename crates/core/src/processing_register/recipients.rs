//! Who receives the company's data, named as the register says it: the
//! service, what it receives the data for, where it stands and who chose it.
//! The platform says where a provider's group is established, never where
//! the company's contract has it process: that is for the company to
//! complete with the guarantees of each transfer.

use super::facts::{
    ChosenBy, ConnectorFact, ConnectorKind, GatewayKind, IdentityFact, ModelService,
};
use crate::identity_providers::ProviderKind;

/// Where a recipient stands, as far as the platform knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A provider established in the European Union.
    EuropeanUnion,
    /// A group established outside the European Union: a transfer to
    /// frame.
    OutsideEuropeanUnion,
    /// A private server the operator lists for the companies' models.
    ListedByOperator,
    /// A server or service the company declared: its own, or its
    /// provider's.
    DeclaredByCompany,
    /// The operator's own infrastructure.
    Operator,
    /// A provider this build does not know.
    Unknown,
}

impl Place {
    pub fn key(self) -> &'static str {
        match self {
            Self::EuropeanUnion => "european_union",
            Self::OutsideEuropeanUnion => "outside_european_union",
            Self::ListedByOperator => "listed_by_operator",
            Self::DeclaredByCompany => "declared_by_company",
            Self::Operator => "operator",
            Self::Unknown => "unknown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::EuropeanUnion => "Fournisseur établi dans l’Union européenne",
            Self::OutsideEuropeanUnion => {
                "Groupe établi hors de l’Union européenne : transfert possible, à encadrer \
                 (décision d’adéquation ou clauses contractuelles types)"
            }
            Self::ListedByOperator => {
                "Serveur privé listé par l’exploitant : localisation selon son hébergement"
            }
            Self::DeclaredByCompany => {
                "Service déclaré par l’entreprise : localisation selon son hébergement ou son \
                 contrat"
            }
            Self::Operator => "Infrastructure de l’exploitant de la plateforme",
            Self::Unknown => "Fournisseur inconnu de la plateforme : localisation à vérifier",
        }
    }
}

impl ChosenBy {
    pub fn key(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Operator => "operator",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Company => "Choisi par l’entreprise",
            Self::Operator => "Fourni par l’exploitant (son sous-traitant)",
        }
    }
}

/// A service that receives data of an activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    pub name: String,
    /// What it receives the data for.
    pub role: String,
    pub place: Place,
    pub chosen_by: ChosenBy,
}

impl Recipient {
    fn new(name: impl Into<String>, role: impl Into<String>, place: Place, by: ChosenBy) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
            place,
            chosen_by: by,
        }
    }
}

/// A model service, for `role`.
pub(super) fn model(service: &ModelService, role: &str) -> Recipient {
    let (name, place) = match (service.provider.as_str(), &service.listed_host) {
        (_, Some(host)) => (
            format!("Serveur de modèles listé par l’exploitant ({host})"),
            Place::ListedByOperator,
        ),
        ("anthropic", None) => ("Anthropic".into(), Place::OutsideEuropeanUnion),
        ("openai", None) => ("OpenAI".into(), Place::OutsideEuropeanUnion),
        ("openrouter", None) => (
            "OpenRouter, et le fournisseur du modèle vers lequel il route".into(),
            Place::OutsideEuropeanUnion,
        ),
        ("gemini", None) => ("Google (Gemini)".into(), Place::OutsideEuropeanUnion),
        ("mistral", None) => ("Mistral AI".into(), Place::EuropeanUnion),
        (other, None) => (format!("Fournisseur « {other} »"), Place::Unknown),
    };
    Recipient::new(name, role, place, service.chosen_by)
}

/// TypeSafe's Jev, for `role` (ADR-213): on the company's key.
pub(super) fn jev(role: &str) -> Recipient {
    Recipient::new(
        "TypeSafe (Jev)",
        role,
        Place::OutsideEuropeanUnion,
        ChosenBy::Company,
    )
}

/// The service a connector's tools reach, if its data leaves the platform.
pub(super) fn connector(fact: &ConnectorFact) -> Option<Recipient> {
    let role = format!("Connecteur « {} »", fact.name);
    let hosts = fact.hosts.join(", ");
    let declared = |what: &str| {
        let name = if hosts.is_empty() {
            what.to_string()
        } else {
            format!("{what} ({hosts})")
        };
        Recipient::new(name, &role, Place::DeclaredByCompany, ChosenBy::Company)
    };
    Some(match fact.kind {
        ConnectorKind::Demo => return None,
        ConnectorKind::McpProcess => Recipient::new(
            "Serveur MCP lancé par la plateforme",
            role,
            Place::Operator,
            ChosenBy::Company,
        ),
        ConnectorKind::Mcp => declared("Serveur MCP"),
        ConnectorKind::Rest => declared("API REST"),
        ConnectorKind::StandardMail => declared("Serveurs de messagerie"),
        ConnectorKind::Other => declared("Service"),
        ConnectorKind::Microsoft365 => Recipient::new(
            "Microsoft 365 (Microsoft Graph)",
            role,
            Place::OutsideEuropeanUnion,
            ChosenBy::Company,
        ),
        ConnectorKind::GoogleWorkspace => Recipient::new(
            "Google Workspace (Gmail, Google Agenda)",
            role,
            Place::OutsideEuropeanUnion,
            ChosenBy::Company,
        ),
    })
}

/// A conversation gateway's service.
pub(super) fn gateway(kind: GatewayKind) -> Recipient {
    let name = match kind {
        GatewayKind::Slack => "Slack",
        GatewayKind::Telegram => "Telegram",
        GatewayKind::Teams => "Microsoft Teams (Bot Framework)",
    };
    Recipient::new(
        name,
        "Passerelle de conversation",
        Place::OutsideEuropeanUnion,
        ChosenBy::Company,
    )
}

/// An identity provider the company signs in with.
pub(super) fn identity_provider(fact: &IdentityFact) -> Recipient {
    let role = format!("Fournisseur d’identité « {} »", fact.name);
    match fact.kind {
        ProviderKind::Microsoft => Recipient::new(
            "Microsoft Entra ID",
            role,
            Place::OutsideEuropeanUnion,
            ChosenBy::Company,
        ),
        ProviderKind::Google => Recipient::new(
            "Google",
            role,
            Place::OutsideEuropeanUnion,
            ChosenBy::Company,
        ),
        ProviderKind::Oidc | ProviderKind::Saml => {
            let protocol = match fact.kind {
                ProviderKind::Saml => "SAML",
                _ => "OpenID Connect",
            };
            let name = match &fact.host {
                Some(host) => format!("Fournisseur {protocol} ({host})"),
                None => format!("Fournisseur {protocol}"),
            };
            Recipient::new(name, role, Place::DeclaredByCompany, ChosenBy::Company)
        }
    }
}

/// The identity provider that provisions accounts by SCIM: it reads them
/// back.
pub(super) fn scim() -> Recipient {
    Recipient::new(
        "Fournisseur d’identité de l’entreprise",
        "Provisionnement SCIM des comptes",
        Place::DeclaredByCompany,
        ChosenBy::Company,
    )
}

/// The company's SIEM, reading its audit log.
pub(super) fn siem() -> Recipient {
    Recipient::new(
        "SIEM de l’entreprise",
        "Export du journal d’audit",
        Place::DeclaredByCompany,
        ChosenBy::Company,
    )
}

/// The operator's relay for the platform's e-mails.
pub(super) fn mail_relay() -> Recipient {
    Recipient::new(
        "Relais d’envoi des e-mails de l’exploitant",
        "Invitations, mots de passe oubliés et avis par e-mail",
        Place::Operator,
        ChosenBy::Operator,
    )
}
