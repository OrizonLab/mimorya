//! PostgreSQL access: migrations, row-level security (one tenant per
//! transaction), repositories per entity and the demo seed.

pub mod client;
mod errors;
pub mod migrate;
pub mod notifications;
pub mod repositories;
pub mod seed;
#[cfg(feature = "testing")]
pub mod testing;

pub use client::{Database, PoolOptions, TenantTx};
pub use errors::is_unique_violation;
pub use migrate::run_migrations;
