//! The generated TypeScript module is the committed one, and the JSON the
//! platform stores reads back as its contract type without losing anything.

use std::path::PathBuf;

use mimorya_agent::runtime::{BudgetLimits, DEFAULT_BUDGET, RunUsageTotals, VerificationReport};
use mimorya_core::tools::Citation as ToolCitation;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

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

/// The engine's sources and checks become the message's metadata unchanged.
#[test]
fn message_metadata_carries_the_engine_s_sources_and_checks() {
    let evidence = [
        ToolCitation::new("intervention", "INT-2026-0142"),
        ToolCitation::new("skill", "planning").labelled("Planification"),
    ];
    let report = VerificationReport {
        passed: true,
        issues: Vec::new(),
        judged: false,
    };
    let metadata = MessageMetadata {
        status: Some(RunStatus::Completed),
        evidence: Some(evidence.iter().map(Into::into).collect()),
        verification: Some((&report).into()),
    };
    let written = serde_json::to_value(&metadata).unwrap();
    assert_eq!(
        written,
        json!({ "status": "completed", "evidence": evidence, "verification": report })
    );
    reads_back::<_, MessageMetadata>(&metadata);
    reads_back::<_, MessageMetadata>(&MessageMetadata::default());
}

#[test]
fn a_stored_value_off_the_contract_is_an_error_that_does_not_quote_it() {
    for stored_value in [
        json!({ "status": "secret" }),
        // A key the contract does not know is not dropped silently.
        json!({ "status": "completed", "sources": ["secret"] }),
        json!({ "evidence": [{ "type": "intervention", "id": "INT-1", "secret": true }] }),
    ] {
        let error = stored::<MessageMetadata>("message metadata", &stored_value)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("secret"), "{error}");
    }
    let usage = json!({ "steps": 1, "toolCalls": 0, "inputTokens": 1, "outputTokens": 1,
                        "costMicros": 0, "cachedTokens": 1 });
    assert!(stored::<RunUsage>("usage", &usage).is_err());
}
