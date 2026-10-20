//! Every event the engine writes to a run's stream reads back as its type.

use mimorya_agent::runtime::RunEvent;
use mimorya_agent::tools::ToolCallEvaluation;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::repositories::runs::RunStatus as DbRunStatus;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::contract::stored::stored;
use crate::contract::*;

/// Every event the engine writes, one of each kind.
fn every_engine_event() -> Vec<RunEvent> {
    let mut events = vec![
        RunEvent::RunStarted {
            objective: "Planifier la semaine".into(),
        },
        RunEvent::ConnectorsUnavailable {
            connectors: vec!["Documentation".into()],
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
    for decision in ToolCallEvaluation::KINDS {
        events.push(RunEvent::ToolCall {
            tool_call_id: "call-2".into(),
            tool_name: "planning_apply".into(),
            title: "Appliquer le planning".into(),
            decision: decision.into(),
            reason: (decision != ToolCallEvaluation::ALLOW).then(|| "Motif".into()),
        });
    }
    for risk in ToolRisk::ALL {
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

/// A new kind of engine event stops compiling here: add it to
/// `every_engine_event` too.
fn _covered(event: &RunEvent) {
    match event {
        RunEvent::RunStarted { .. }
        | RunEvent::ConnectorsUnavailable { .. }
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
