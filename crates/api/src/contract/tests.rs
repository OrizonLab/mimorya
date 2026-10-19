//! The generated TypeScript module is the committed one, and the JSON the
//! platform stores reads back as its contract type without losing anything.

use std::path::PathBuf;

use mimorya_agent::runtime::{
    BudgetLimits, DEFAULT_BUDGET, RunEvent, RunUsageTotals, VerificationReport,
};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::memory::MemoryKind as CoreMemoryKind;
use mimorya_core::tools::Citation as ToolCitation;
use mimorya_db::repositories::runs::RunStatus as DbRunStatus;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use super::stored::{stored, stored_text};
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

/// Every event the engine writes, one of each kind.
fn every_engine_event() -> Vec<RunEvent> {
    let mut events = vec![
        RunEvent::RunStarted {
            objective: "Planifier la semaine".into(),
        },
        RunEvent::AssistantMessage {
            text: "Je regarde le planning.".into(),
            is_final: false,
        },
        RunEvent::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: "interventions_search".into(),
            summary: "3 interventions".into(),
            is_error: false,
            duration_ms: 42,
        },
        RunEvent::ApprovalResolved {
            approval_id: Uuid::new_v4(),
            approved: true,
        },
        RunEvent::Verification {
            attempt: 1,
            passed: false,
            issues: vec!["Source manquante".into()],
        },
        RunEvent::Usage {
            input_tokens: 1_000,
            output_tokens: 200,
            cost_micros: None,
        },
    ];
    for (decision, reason) in [
        ("allow", None),
        ("require_approval", Some("Risque")),
        ("deny", Some("Refus")),
    ] {
        events.push(RunEvent::ToolCall {
            tool_call_id: "call-2".into(),
            tool_name: "planning_apply".into(),
            title: "Appliquer le planning".into(),
            decision: decision.into(),
            reason: reason.map(Into::into),
        });
    }
    for risk in [
        ToolRisk::Read,
        ToolRisk::Draft,
        ToolRisk::Write,
        ToolRisk::External,
        ToolRisk::Irreversible,
    ] {
        events.push(RunEvent::ApprovalRequested {
            approval_id: Uuid::new_v4(),
            tool_name: "planning_apply".into(),
            action_summary: "Programmer INT-2049".into(),
            risk,
        });
    }
    for status in DbRunStatus::ALL {
        events.push(RunEvent::RunFinished {
            status: status.as_str().into(),
            error: (status == DbRunStatus::Failed).then(|| "Erreur interne".into()),
        });
    }
    events
}

/// No engine event is left out of `every_engine_event` (compile-time check).
fn _covered(event: &RunEvent) {
    match event {
        RunEvent::RunStarted { .. }
        | RunEvent::AssistantMessage { .. }
        | RunEvent::ToolCall { .. }
        | RunEvent::ToolResult { .. }
        | RunEvent::ApprovalRequested { .. }
        | RunEvent::ApprovalResolved { .. }
        | RunEvent::Verification { .. }
        | RunEvent::Usage { .. }
        | RunEvent::RunFinished { .. } => {}
    }
}

#[test]
fn every_event_of_the_stream_reads_back_as_its_contract_type() {
    let mut streamed: Vec<(String, Value)> = every_engine_event()
        .into_iter()
        .map(RunEvent::into_parts)
        .collect();
    let delta = serde_json::to_value(AssistantDelta { text: "Je ".into() }).unwrap();
    streamed.push(("assistant_delta".into(), delta));
    for (name, data) in streamed {
        let event = json!({ "type": name, "data": data });
        let read: RunStreamEvent = stored("event", &event).expect("event matches the contract");
        assert_eq!(serde_json::to_value(read).unwrap(), event);
    }
}

#[test]
fn memory_columns_read_back_as_their_contract_values() {
    for scope in ["user", "org_unit", "tenant", "experience"] {
        let read: MemoryScope = stored_text("memory scope", scope).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), scope);
    }
    for kind in CoreMemoryKind::ALL {
        let read: MemoryKind = stored_text("memory kind", kind.as_str()).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), kind.as_str());
    }
    for status in ["active", "proposed", "archived"] {
        let read: MemoryStatus = stored_text("memory status", status).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), status);
    }
    let error = stored_text::<MemoryScope>("memory scope", "secret").unwrap_err();
    assert!(!error.to_string().contains("secret"));
}
