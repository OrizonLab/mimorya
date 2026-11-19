//! Agent engine: provider-neutral LLM layer, tool policy enforcement, the run
//! loop with human validations, answer verification and budgets. It depends
//! on ports (`runtime::ports`), never on the database. Its decision service
//! makes quick, typed judgements for it and the platform (`decision`).

pub mod decision;
pub mod llm;
pub mod runtime;
#[cfg(feature = "testing")]
pub mod testing;
pub mod tools;
