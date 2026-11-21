//! What the register is drawn from: the company's settings as they stand
//! when it is asked for, read by the API. Nothing here is stored.

use chrono::{DateTime, Utc};

use crate::gateways::GatewayKind;
use crate::identity_providers::ProviderKind;
use crate::retention::RetentionKind;
use crate::second_factor::Requirement;

/// Who chose a service the company's data goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenBy {
    /// The company: its own key, application, bot or server.
    Company,
    /// The operator, who supplies it (its model on its key, ADR-208; its
    /// relay for e-mails): one of its own processors.
    Operator,
}

/// A model service: of the conversations, of the review of their answers,
/// or of embeddings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelService {
    /// Its kind as the platform names it (`anthropic`, `mistral`,
    /// `ollama`…).
    pub provider: String,
    /// The host of the server it is reached at when that is not its
    /// provider's own: an endpoint the operator lists for a company's local
    /// kind (`ollama`, `openai_compatible`), or the one the operator set for
    /// its own model (`LLM_BASE_URL`).
    pub server_host: Option<String>,
    pub chosen_by: ChosenBy,
}

/// What a connector of the company is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    /// Fictitious data, inside the platform.
    Demo,
    /// An MCP server reached over HTTP.
    Mcp,
    /// An MCP server the platform starts itself, as a process.
    McpProcess,
    Rest,
    Microsoft365,
    GoogleWorkspace,
    StandardMail,
    /// A type this build does not describe.
    Other,
}

/// An enabled connector of the company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorFact {
    pub kind: ConnectorKind,
    pub name: String,
    /// The hosts its tools reach, as declared (none for an MCP server the
    /// platform starts itself, nor an address it cannot read).
    pub hosts: Vec<String>,
}

/// The operator's relay for the platform's e-mails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailRelay {
    /// Its host, when the platform knows it.
    pub host: Option<String>,
}

/// An enabled identity provider of the company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFact {
    pub kind: ProviderKind,
    pub name: String,
    /// The host of its issuer, when it is an address.
    pub host: Option<String>,
}

/// A duration retention applies, as the purge applies it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionFact {
    pub kind: RetentionKind,
    pub in_effect: i32,
    /// The company's choice, or the platform's without one.
    pub chosen: i32,
    /// When a shortening that waits applies.
    pub shortening_at: Option<DateTime<Utc>>,
}

/// What the company's decision service is asked about (ADR-213): the
/// usages it set and the platform delivers, not off. Its main model decides
/// them, or Jev first when it chose Jev and Jev opens, the main model then
/// deciding what Jev does not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecisionUses {
    /// Choosing a run's model or its skills, from the request (D1, D2).
    pub requests: bool,
    /// Sensitive data in a memory or a lesson (D3).
    pub memories: bool,
    /// A deposited document's type and confidentiality (D4).
    pub documents: bool,
    /// An action's parameters, before it goes or is put to validation (D5).
    pub actions: bool,
    /// The beginning of an external content a tool brings (D6): a
    /// connector's, a deposited document's passages.
    pub external_contents: bool,
}

impl DecisionUses {
    pub fn any(self) -> bool {
        self.requests || self.memories || self.documents || self.actions || self.external_contents
    }
}

/// The company's settings the register is drawn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterFacts {
    /// The company's name: the controller.
    pub company: String,
    /// The model of its runs, if they have one.
    pub runs_model: Option<ModelService>,
    /// The model it chose apart to review answers (ADR-108).
    pub review_model: Option<ModelService>,
    /// Its embedding model (ADR-074).
    pub embeddings: Option<ModelService>,
    pub decisions: DecisionUses,
    /// Jev is chosen, opened by the operator and its key stored: TypeSafe
    /// receives what the decisions ask.
    pub jev: bool,
    pub connectors: Vec<ConnectorFact>,
    /// Its enabled conversation gateways.
    pub gateways: Vec<GatewayKind>,
    pub identity_providers: Vec<IdentityFact>,
    /// Its identity provider holds a SCIM token (ADR-183).
    pub scim: bool,
    /// Its SIEM holds an export token of the audit log (ADR-181).
    pub audit_export: bool,
    /// The relay the platform sends e-mails through, if it sends any.
    pub mail_relay: Option<MailRelay>,
    pub retention: Vec<RetentionFact>,
    pub second_factor: Requirement,
}
