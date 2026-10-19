//! Run records and message metadata, as the engine and the platform write
//! them, and memory columns, as the core names their values.

use mimorya_agent::runtime::{BudgetLimits, DEFAULT_BUDGET, RunUsageTotals, VerificationReport};
use mimorya_core::memory::{
    MemoryKind as CoreMemoryKind, MemoryScope as CoreMemoryScope, MemoryStatus as CoreMemoryStatus,
};
use mimorya_core::tools::Citation as ToolCitation;
use serde_json::json;

use super::reads_back;
use crate::contract::stored::{stored, stored_text};
use crate::contract::*;

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

#[test]
fn memory_columns_read_back_as_their_contract_values() {
    for scope in CoreMemoryScope::ALL {
        let read: MemoryScope = stored_text("memory scope", scope.as_str()).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), scope.as_str());
    }
    for kind in CoreMemoryKind::ALL {
        let read: MemoryKind = stored_text("memory kind", kind.as_str()).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), kind.as_str());
    }
    for status in CoreMemoryStatus::ALL {
        let read: MemoryStatus = stored_text("memory status", status.as_str()).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), status.as_str());
    }
    let error = stored_text::<MemoryScope>("memory scope", "secret").unwrap_err();
    assert!(!error.to_string().contains("secret"));
}
