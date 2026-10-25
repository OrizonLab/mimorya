//! Test doubles: a scripted model, an embedding model of concepts and
//! in-memory ports (documents and sub-agents' runs included). Never used in
//! production.

mod access;
mod concept_embedder;
mod in_memory_documents;
mod in_memory_memory;
mod in_memory_ports;
mod in_memory_skills;
mod in_memory_sub_runs;
mod scripted;

pub use access::TestAccess;
pub use concept_embedder::ConceptEmbedder;
pub use in_memory_documents::DocumentFixture;
pub use in_memory_memory::memory_fixture;
pub use in_memory_ports::{FinishedRecord, InMemoryPorts, StoredMemory};
pub use in_memory_skills::SkillFixture;
pub use in_memory_sub_runs::OpenedSubRun;
pub use scripted::{
    RecordedRequest, ScriptedCall, ScriptedProvider, ScriptedReply, ScriptedStep, call,
    last_tool_results,
};
