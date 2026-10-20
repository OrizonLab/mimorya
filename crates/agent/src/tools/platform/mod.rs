//! Built-in platform tools available to every agent run (memory and skills).

mod memory_tools;
mod skill_tools;

use std::sync::Arc;

use mimorya_core::tools::{ToolRef, typed_tool};

pub use memory_tools::{MemoryRemember, MemorySearch};
pub use skill_tools::{SKILL_READ, SkillRead};

use crate::runtime::ports::{MemoryStore, SkillStore};

/// The memory tools read the rules from the call's authorization: the rights
/// in force at that step (ADR-035).
pub fn platform_tools(memory: Arc<dyn MemoryStore>, skills: Arc<dyn SkillStore>) -> Vec<ToolRef> {
    vec![
        typed_tool(MemorySearch {
            memory: memory.clone(),
        }),
        typed_tool(MemoryRemember { memory }),
        typed_tool(SkillRead { skills }),
    ]
}
