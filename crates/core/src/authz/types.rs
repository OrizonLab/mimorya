//! Authorization data model: subjects, actors, role assignments, resources and
//! decisions.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use super::conditions::Condition;
use super::sensitivity::Sensitivity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssignmentScope {
    Tenant,
    OrgUnit {
        #[serde(rename = "orgUnitId")]
        org_unit_id: Uuid,
        #[serde(rename = "includeDescendants")]
        include_descendants: bool,
    },
    /// Resources owned by the user only.
    #[serde(rename = "self")]
    Own,
}

impl AssignmentScope {
    pub fn subtree(org_unit_id: Uuid) -> Self {
        Self::OrgUnit {
            org_unit_id,
            include_descendants: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    /// Permission patterns (`domain:action`, `domain:*` or `*`).
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAssignment {
    pub role_id: Uuid,
    pub scope: AssignmentScope,
    pub expires_at: Option<DateTime<Utc>>,
}

impl RoleAssignment {
    pub fn new(role_id: Uuid, scope: AssignmentScope) -> Self {
        Self {
            role_id,
            scope,
            expires_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    /// Org units the user belongs to (organisational context, not a grant).
    pub org_unit_ids: Vec<Uuid>,
    pub assignments: Vec<RoleAssignment>,
    /// Highest sensitivity level the user may read (`internal` by default).
    pub clearance: Option<Sensitivity>,
    pub attributes: Map<String, Value>,
}

impl Subject {
    pub fn clearance(&self) -> Sensitivity {
        self.clearance.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }
}

/// The channel of a run its author started from the application.
pub const WEB_CHANNEL: &str = "web";
/// The channel of a run another system started through its author's inbound
/// hook (ADR-109): nobody watches it, and it was sent untrusted data.
pub const HOOK_CHANNEL: &str = "hook";
/// The channel of a run its author started by writing to their company's
/// Slack application (ADR-127): their own words, answered in Slack.
pub const SLACK_CHANNEL: &str = "slack";
/// The channel of a run its author started by writing to their company's
/// Telegram bot (ADR-135): their own words, answered in Telegram.
pub const TELEGRAM_CHANNEL: &str = "telegram";

/// Who is performing the action: the user directly, or an agent on their behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub kind: ActorType,
    pub run_id: Option<Uuid>,
    /// For agents only: an optional allow-list narrowing what the run may use
    /// (sub-agents, restricted missions). It can only narrow, never extend.
    pub allowed_permissions: Option<HashSet<String>>,
    pub channel: Option<String>,
}

impl Actor {
    pub fn user() -> Self {
        Self {
            kind: ActorType::User,
            run_id: None,
            allowed_permissions: None,
            channel: None,
        }
    }

    pub fn agent(run_id: Option<Uuid>) -> Self {
        Self {
            kind: ActorType::Agent,
            run_id,
            allowed_permissions: None,
            channel: None,
        }
    }

    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    /// Whether the actor is a run another system started (ADR-109).
    pub fn started_by_hook(&self) -> bool {
        self.channel.as_deref() == Some(HOOK_CHANNEL)
    }

    pub fn restricted_to<I: IntoIterator<Item = S>, S: Into<String>>(
        mut self,
        permissions: I,
    ) -> Self {
        self.allowed_permissions = Some(permissions.into_iter().map(Into::into).collect());
        self
    }

    pub fn is_agent(&self) -> bool {
        self.kind == ActorType::Agent
    }
}

/// A resource submitted to an authorization check.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceRef {
    pub resource_type: String,
    pub id: Option<String>,
    pub tenant_id: Uuid,
    pub org_unit_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub sensitivity: Option<Sensitivity>,
    pub attributes: Map<String, Value>,
}

impl ResourceRef {
    pub fn new(resource_type: impl Into<String>, tenant_id: Uuid) -> Self {
        Self {
            resource_type: resource_type.into(),
            id: None,
            tenant_id,
            org_unit_id: None,
            owner_user_id: None,
            sensitivity: None,
            attributes: Map::new(),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn org_unit(mut self, org_unit_id: Option<Uuid>) -> Self {
        self.org_unit_id = org_unit_id;
        self
    }

    pub fn owner(mut self, owner_user_id: Option<Uuid>) -> Self {
        self.owner_user_id = owner_user_id;
        self
    }

    pub fn sensitivity(mut self, sensitivity: Option<Sensitivity>) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbacPolicy {
    pub id: Uuid,
    pub description: String,
    pub effect: PolicyEffect,
    pub permissions: Vec<String>,
    pub condition: Condition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataScope {
    None,
    All,
    Restricted {
        org_unit_ids: HashSet<Uuid>,
        /// Resources owned by these users are in scope (from `self` assignments).
        owner_user_ids: HashSet<Uuid>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
    pub policy_id: Option<Uuid>,
}

impl Decision {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: reason.into(),
            policy_id: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: reason.into(),
            policy_id: None,
        }
    }
}
