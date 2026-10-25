//! Built-in platform tools available to every agent run (memory, documents,
//! skills and delegation to sub-agents).

mod delegate_tool;
mod document_tools;
mod memory_tools;
mod skill_tools;

use std::sync::Arc;

use mimorya_core::tools::{ToolRef, typed_tool};

pub use delegate_tool::{DELEGATE_TASK, DelegateTask, DelegateTaskInput};
pub use document_tools::{DocumentRead, DocumentSearch, READ_PASSAGES};
pub use memory_tools::{MemoryRemember, MemorySearch};
pub use skill_tools::{SKILL_READ, SkillRead};

use crate::runtime::DocumentStore;
use crate::runtime::ports::{MemoryStore, SkillStore};

/// The memory and document tools read the rules from the call's
/// authorization: the rights in force at that step (ADR-035).
pub fn platform_tools(
    memory: Arc<dyn MemoryStore>,
    skills: Arc<dyn SkillStore>,
    documents: Arc<dyn DocumentStore>,
) -> Vec<ToolRef> {
    vec![
        typed_tool(MemorySearch {
            memory: memory.clone(),
        }),
        typed_tool(MemoryRemember { memory }),
        typed_tool(DocumentSearch {
            documents: documents.clone(),
        }),
        typed_tool(DocumentRead { documents }),
        typed_tool(SkillRead { skills }),
        typed_tool(DelegateTask),
    ]
}
