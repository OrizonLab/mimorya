//! The processing activities the platform carries out for a company, each
//! with its purpose, the people and data it concerns, who receives them and
//! how long they are kept. The texts are fixed; the recipients and the
//! durations are the company's own.

mod agent;
mod platform;
mod reach;

use super::facts::RegisterFacts;
use super::recipients::Recipient;

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
/// What the collaborators write there: the platform never looks for
/// sensitive data in it.
const TYPED: &str = "Les collaborateurs peuvent y écrire des données sensibles (santé, opinions, \
    appartenance syndicale…) : la plateforme ne les y cherche pas ; à encadrer par la charte \
    d’usage de l’entreprise.";

/// The activities of the company, in the register's order: that of the
/// connectors when one is enabled or keeps something (ADR-220), that of the
/// gateways when one is enabled.
pub(super) fn activities(facts: &RegisterFacts) -> Vec<Activity> {
    let mut all = vec![
        platform::accounts(facts),
        agent::assistance(facts),
        agent::memory(facts),
        agent::documents(facts),
    ];
    if facts.connectors.iter().any(reach::processed) {
        all.push(reach::connectors(facts));
    }
    if !facts.gateways.is_empty() {
        all.push(reach::gateways(facts));
    }
    all.push(platform::audit(facts));
    all.push(platform::usage());
    all
}
