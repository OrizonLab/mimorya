//! Built-in platform tools available to every agent run (memory and skills).

mod memory_tools;
mod skill_tools;

use std::sync::Arc;

use mimorya_core::authz::AuthorizationEngine;
use mimorya_core::tools::{ToolRef, typed_tool};

pub use memory_tools::{MemoryRemember, MemorySearch};
pub use skill_tools::SkillRead;

use crate::runtime::ports::{MemoryStore, SkillStore};

pub fn platform_tools(
    engine: Arc<AuthorizationEngine>,
    memory: Arc<dyn MemoryStore>,
    skills: Arc<dyn SkillStore>,
) -> Vec<ToolRef> {
    vec![
        typed_tool(MemorySearch {
            engine: engine.clone(),
            memory: memory.clone(),
        }),
        typed_tool(MemoryRemember { engine, memory }),
        typed_tool(SkillRead { skills }),
    ]
}
