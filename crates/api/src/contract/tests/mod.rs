//! The generated TypeScript module is the committed one, and what the
//! platform stores reads back as its contract type without losing anything.

mod admin;
mod records;
mod stream;

use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::stored::{stored, stored_text};
use super::typescript;

fn contract_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/lib/api/contract.ts")
}

/// `pnpm contract` writes the file instead (`MIMORYA_WRITE_CONTRACT=1`).
#[test]
fn the_web_client_s_types_are_generated_from_the_contract() {
    let generated = typescript();
    if std::env::var_os("MIMORYA_WRITE_CONTRACT").is_some() {
        std::fs::write(contract_file(), &generated).expect("contract.ts written");
        return;
    }
    let committed = std::fs::read_to_string(contract_file()).unwrap_or_default();
    let same_lines = generated
        .lines()
        .zip(committed.lines())
        .take_while(|(expected, found)| expected == found)
        .count();
    assert!(
        committed == generated,
        "apps/web/lib/api/contract.ts differs from the contract from line {}: run `pnpm contract`",
        same_lines + 1
    );
}

/// What the platform stores reads back as `T`, and is answered unchanged.
pub(super) fn reads_back<S: Serialize, T: DeserializeOwned + Serialize>(written: &S) {
    let value = serde_json::to_value(written).unwrap();
    let read: T = stored("test", &value).expect("stored value matches the contract");
    assert_eq!(serde_json::to_value(read).unwrap(), value);
}

/// Each value of a reference list reads back as the contract's value.
pub(super) fn every_value_reads_back<T: DeserializeOwned + Serialize>(
    column: &str,
    values: &[&str],
) {
    for value in values {
        let read: T = stored_text(column, value).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), *value, "{column}");
    }
}
