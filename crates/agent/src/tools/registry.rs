//! The tools available to one tenant, after administrator overrides. Tools are
//! filtered per subject before being shown to a model.

use std::collections::HashMap;

use anyhow::bail;
use mimorya_core::authz::{Actor, AuthorizationEngine, Subject};
use mimorya_core::autonomy::{AutonomyPolicy, ToolRisk, ToolRiskProfile, decide_execution};
use mimorya_core::tools::{ToolInfo, ToolRef};
use serde::{Deserialize, Serialize};

use crate::llm::LlmToolSpec;

/// Administrator overrides for a tool (from connector configuration).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOverride {
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub risk: Option<ToolRisk>,
    pub requires_approval: Option<bool>,
    /// What the model reads of the tool instead of its own description
    /// (ADR-057); its name, schema and policy stay as they are.
    pub description: Option<String>,
}

/// A tool with the description in force for the tenant.
#[derive(Clone)]
pub struct RegisteredTool {
    pub info: ToolInfo,
    pub tool: ToolRef,
}

impl RegisteredTool {
    pub fn risk_profile(&self) -> ToolRiskProfile<'_> {
        ToolRiskProfile {
            name: &self.info.name,
            risk: self.info.risk,
            requires_approval: self.info.requires_approval,
        }
    }
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    /// In registration order.
    tools: Vec<RegisteredTool>,
}

impl ToolRegistry {
    pub fn new(
        tools: Vec<ToolRef>,
        overrides: &HashMap<String, ToolOverride>,
    ) -> anyhow::Result<Self> {
        let mut registry = Self::default();
        for tool in tools {
            let name = tool.info().name.clone();
            registry.register(tool, overrides.get(&name))?;
        }
        Ok(registry)
    }

    pub fn register(
        &mut self,
        tool: ToolRef,
        override_: Option<&ToolOverride>,
    ) -> anyhow::Result<()> {
        let mut info = tool.info().clone();
        if self.get(&info.name).is_some() {
            bail!("Duplicate tool {}", info.name);
        }
        if let Some(o) = override_ {
            if o.enabled == Some(false) {
                return Ok(());
            }
            if let Some(permissions) = &o.permissions {
                info.required_permissions = permissions.clone();
            }
            if let Some(risk) = o.risk {
                info.risk = risk;
            }
            if let Some(requires_approval) = o.requires_approval {
                info.requires_approval = requires_approval;
            }
            if let Some(description) = &o.description {
                info.description = description.clone();
            }
        }
        self.tools.push(RegisteredTool { info, tool });
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.iter().find(|t| t.info.name == name)
    }

    pub fn all(&self) -> &[RegisteredTool] {
        &self.tools
    }

    /// Tools the agent may offer to the model for this subject: required
    /// permissions held (agent ceiling applied) and not forbidden by autonomy.
    pub fn visible_to(
        &self,
        engine: &AuthorizationEngine,
        subject: &Subject,
        actor: &Actor,
        autonomy: &AutonomyPolicy,
    ) -> Vec<&RegisteredTool> {
        self.tools
            .iter()
            .filter(|t| {
                t.info
                    .required_permissions
                    .iter()
                    .all(|p| engine.has(subject, p, actor))
            })
            .filter(|t| !decide_execution(autonomy, t.risk_profile()).is_deny())
            .collect()
    }
}

pub fn to_tool_spec(tool: &RegisteredTool) -> LlmToolSpec {
    LlmToolSpec {
        name: tool.info.name.clone(),
        description: tool.info.description.clone(),
        input_schema: tool.tool.input_schema(),
    }
}
