//! Test doubles: a scripted model, an embedding model of concepts and
//! in-memory ports (documents included). Never used in production.

mod access;
mod concept_embedder;
mod in_memory_documents;
mod in_memory_memory;
mod in_memory_ports;
mod scripted;

pub use access::TestAccess;
pub use concept_embedder::ConceptEmbedder;
pub use in_memory_documents::DocumentFixture;
pub use in_memory_memory::memory_fixture;
pub use in_memory_ports::{FinishedRecord, InMemoryPorts, SkillFixture, StoredMemory};
pub use scripted::{
    RecordedRequest, ScriptedCall, ScriptedProvider, ScriptedReply, ScriptedStep, call,
    last_tool_results,
};
