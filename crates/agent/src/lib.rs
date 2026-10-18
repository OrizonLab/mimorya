//! Agent engine: provider-neutral LLM layer, tool policy enforcement, the run
//! loop with human validations, answer verification and budgets. It depends
//! on ports (`runtime::ports`), never on the database.

pub mod llm;
pub mod runtime;
#[cfg(feature = "testing")]
pub mod testing;
pub mod tools;
