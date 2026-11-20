//! A result of a tool that brings external content, screened for
//! instructions meant for an AI before the model reads it (ADR-213, D6):
//! said to hold some, the run is held suspect and the result marked; what
//! the decision cost counted in the run.

use mimorya_core::tools::ToolResult;
use serde_json::Value;

use super::ToolCallProcessor;
use crate::runtime::content_screen::{ScreenVerdict, screened};
use crate::runtime::decision_costs::count;
use crate::runtime::state::RunCheckpoint;
use crate::tools::RegisteredTool;

impl ToolCallProcessor {
    /// Whether `result` of `tool` is suspect, to be marked so; the run is
    /// held suspect when it is, or when it could not be read.
    pub(super) async fn screen(
        &self,
        state: &mut RunCheckpoint,
        tool: &RegisteredTool,
        result: &ToolResult,
    ) -> anyhow::Result<bool> {
        if !screened(tool) {
            return Ok(false);
        }
        let text = match &result.content {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let review = self
            .ports
            .content_screen
            .screen(&tool.info.name, &text)
            .await;
        count(&self.ports, state, &review.counted).await?;
        Ok(match review.verdict {
            ScreenVerdict::Clear => false,
            ScreenVerdict::Suspect => {
                state.external_suspect = true;
                true
            }
            ScreenVerdict::Unread => {
                state.external_suspect = true;
                false
            }
        })
    }
}
