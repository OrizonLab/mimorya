//! PostgreSQL access: migrations, row-level security (one tenant per
//! transaction), repositories per entity and the demo seed.

pub mod client;
pub mod migrate;
pub mod repositories;
pub mod seed;
#[cfg(feature = "testing")]
pub mod testing;

pub use client::{Database, PoolOptions, TenantTx};
pub use migrate::run_migrations;
