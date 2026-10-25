//! The tools available to one tenant, after administrator overrides. Tools are
//! filtered per subject before being shown to a model.

use std::collections::HashMap;

use anyhow::bail;
use mimorya_core::authz::{Actor, AuthorizationEngine, Subject};
use mimorya_core::autonomy::{AutonomyPolicy, ToolRisk, ToolRiskProfile, decide_execution};
use mimorya_core::tools::{ToolInfo, ToolRef};
use serde::{Deserialize, Serialize};

use super::platform::DELEGATE_TASK;
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

    /// The same tools as the company set them, those named in `tools` run
    /// by these instances: a sub-agent's platform tools report through its
    /// own run (ADR-098).
    pub fn rebound(&self, tools: Vec<ToolRef>) -> Self {
        let mut own: HashMap<String, ToolRef> = tools
            .into_iter()
            .map(|tool| (tool.info().name.clone(), tool))
            .collect();
        let tools = self
            .tools
            .iter()
            .map(|t| RegisteredTool {
                info: t.info.clone(),
                tool: own.remove(&t.info.name).unwrap_or_else(|| t.tool.clone()),
            })
            .collect();
        Self { tools }
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.iter().find(|t| t.info.name == name)
    }

    pub fn all(&self) -> &[RegisteredTool] {
        &self.tools
    }

    /// The tools a sub-agent may be given (ADR-098): those that only read
    /// and run without validation, as the company set them, delegation
    /// aside.
    pub fn for_sub_agents(&self) -> Self {
        let tools = self
            .tools
            .iter()
            .filter(|t| t.info.risk == ToolRisk::Read && !t.info.requires_approval)
            .filter(|t| t.info.name != DELEGATE_TASK)
            .cloned()
            .collect();
        Self { tools }
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
