//! Rebuilds the crate when a migration is added or changed: `sqlx::migrate!`
//! embeds the SQL files at build time, and Cargo does not track them otherwise.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
