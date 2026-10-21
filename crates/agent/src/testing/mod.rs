//! Test doubles: a scripted model and in-memory ports. Never used in production.

mod access;
mod in_memory_memory;
mod in_memory_ports;
mod scripted;

pub use access::TestAccess;
pub use in_memory_memory::memory_fixture;
pub use in_memory_ports::{FinishedRecord, InMemoryPorts, SkillFixture, StoredMemory};
pub use scripted::{
    RecordedRequest, ScriptedCall, ScriptedProvider, ScriptedReply, ScriptedStep, call,
    last_tool_results,
};
