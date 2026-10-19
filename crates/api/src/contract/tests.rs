//! The generated TypeScript module is the committed one, and the JSON the
//! platform stores reads back as its contract type without losing anything.

use std::path::PathBuf;

use mimorya_agent::runtime::{BudgetLimits, DEFAULT_BUDGET, RunUsageTotals, VerificationReport};
use mimorya_core::tools::Citation as ToolCitation;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::stored::stored;
use super::*;

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
    assert!(
        committed == generated,
        "apps/web/lib/api/contract.ts is not generated from the current contract: run `pnpm contract`"
    );
}

/// What the platform stores reads back as `T`, and is answered unchanged.
fn reads_back<S: Serialize, T: DeserializeOwned + Serialize>(written: &S) {
    let value = serde_json::to_value(written).unwrap();
    let read: T = stored("test", &value).expect("stored value matches the contract");
    assert_eq!(serde_json::to_value(read).unwrap(), value);
}

#[test]
fn run_records_read_back_as_their_contract_types() {
    reads_back::<_, RunUsage>(&RunUsageTotals {
        steps: 3,
        tool_calls: 2,
        input_tokens: 1_200,
        output_tokens: 300,
        cost_micros: 4_500,
    });
    reads_back::<BudgetLimits, RunBudget>(&DEFAULT_BUDGET);
    reads_back::<_, Verification>(&VerificationReport {
        passed: false,
        issues: vec!["L’identifiant INT-1 n’apparaît dans aucun résultat.".into()],
        judged: true,
    });
}

#[test]
fn message_metadata_reads_back_as_its_contract_type() {
    let evidence = vec![
        ToolCitation::new("intervention", "INT-2026-0142"),
        ToolCitation::new("skill", "planning").labelled("Planification"),
    ];
    let report = VerificationReport {
        passed: true,
        issues: Vec::new(),
        judged: false,
    };
    for metadata in [
        json!({}),
        json!({ "status": "failed" }),
        json!({ "status": "completed", "evidence": evidence, "verification": report }),
    ] {
        reads_back::<Value, MessageMetadata>(&metadata);
    }
}

#[test]
fn a_stored_value_off_the_contract_is_an_error_that_does_not_quote_it() {
    let error = stored::<MessageMetadata>("message metadata", &json!({ "status": "secret" }))
        .unwrap_err()
        .to_string();
    assert!(!error.contains("secret"), "{error}");
}
