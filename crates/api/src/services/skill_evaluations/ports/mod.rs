//! The ports of a case played for an evaluation (ADR-104): what it reads
//! (`knowledge.rs`), what it records (`journal.rs`), and what it is never
//! given (`withheld.rs`). A case is no run: nothing of it is written to the
//! runs' tables.

mod journal;
mod knowledge;
mod withheld;

use std::sync::Arc;

use mimorya_agent::runtime::EnginePorts;
use uuid::Uuid;

pub use journal::{Journal, ToolTrace};
pub use knowledge::{EvaluatedVersion, Knowledge};

use crate::app::Services;
use withheld::Withheld;

/// Whose case it is, in which company and evaluation.
#[derive(Debug, Clone, Copy)]
pub struct CaseScope {
    pub tenant_id: Uuid,
    pub evaluation_id: Uuid,
    /// Whom the case runs for: who asked for the evaluation.
    pub user_id: Uuid,
}

/// The ports, and what the case's result keeps of them.
pub struct CasePorts {
    pub ports: EnginePorts,
    pub journal: Arc<Journal>,
    pub knowledge: Arc<Knowledge>,
}

pub fn case_ports(services: &Services, scope: CaseScope, evaluated: EvaluatedVersion) -> CasePorts {
    let journal = Arc::new(Journal::new(services.db.clone(), scope));
    let knowledge = Arc::new(Knowledge::new(
        services.db.clone(),
        services.embeddings.clone(),
        scope,
        evaluated,
    ));
    let withheld = Arc::new(Withheld);
    let ports = EnginePorts {
        events: journal.clone(),
        audit: journal.clone(),
        tool_log: journal.clone(),
        approvals: withheld.clone(),
        usage: journal.clone(),
        memory: knowledge.clone(),
        documents: knowledge.clone(),
        skills: knowledge.clone(),
        runs: journal.clone(),
        deliverables: withheld.clone(),
        sub_runs: withheld,
    };
    CasePorts {
        ports,
        journal,
        knowledge,
    }
}
