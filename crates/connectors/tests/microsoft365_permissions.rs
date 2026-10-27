//! The permissions each Microsoft 365 tool needs of the person's link
//! (ADR-119): reading mail, reading the calendar, sending, writing the
//! calendar. A link that does not grant one is refused before Graph, and
//! the person is told to link their account again; each other tool works.

mod support;

use std::sync::Arc;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::testing::FakeGraph;
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, ToolResult};
use serde_json::{Value, json};
use support::linked_accounts::{call, mailbox_executor, validated_call};
use support::microsoft365::microsoft365_writing_tools;
use uuid::Uuid;

async fn granting(graph: &FakeGraph, granted: &str) -> ToolExecutor {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account_granting(MICROSOFT365, "acces-lea-1", granted);
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let tools = microsoft365_writing_tools(&graph.url).await;
    mailbox_executor(tools, user::LEA, &ports, accounts)
}

/// Each tool, called as a run would: the writing ones validated.
async fn used(executor: &ToolExecutor, tool: &str) -> ToolResult {
    let input: Value = match tool {
        "m365_mail_search" => json!({}),
        "m365_mail_read" => json!({ "id": "lea/m1=" }),
        "m365_calendar_events" => json!({ "from": "2026-10-26", "to": "2026-10-30" }),
        "m365_mail_send" => {
            json!({ "to": ["paul@exemple.fr"], "subject": "Point", "body": "Bonjour." })
        }
        _ => json!({ "subject": "Point", "start": "2026-11-03T09:30:00", "durationMinutes": 30 }),
    };
    match tool {
        "m365_mail_send" | "m365_calendar_event_create" => {
            validated_call(executor, tool, input).await
        }
        _ => call(executor, tool, input).await,
    }
}

#[tokio::test]
async fn each_tool_needs_its_own_permission() {
    const TOOLS: [&str; 5] = [
        "m365_mail_search",
        "m365_mail_read",
        "m365_calendar_events",
        "m365_mail_send",
        "m365_calendar_event_create",
    ];
    // What a link grants, and which of the five tools it serves.
    let links = [
        (
            "openid offline_access Mail.Read",
            [true, true, false, false, false],
        ),
        (
            "Calendars.Read https://graph.microsoft.com/Mail.Send",
            [false, false, true, true, false],
        ),
        (
            "Mail.Read Calendars.ReadWrite",
            [true, true, false, false, true],
        ),
    ];
    for (granted, served) in links {
        let graph = FakeGraph::start().await;
        let executor = granting(&graph, granted).await;
        for (tool, serves) in TOOLS.into_iter().zip(served) {
            let result = used(&executor, tool).await;
            if serves {
                assert!(!result.is_error, "{granted} / {tool}: {:?}", result.content);
                continue;
            }
            assert!(result.is_error, "{granted} / {tool}");
            let told = result.content["erreur"].as_str().unwrap_or_default();
            assert!(
                told.contains("relie de nouveau son compte"),
                "{tool}: {told}"
            );
        }
        // Only what the link serves reached Graph.
        let reached = served.iter().filter(|serves| **serves).count();
        assert_eq!(graph.requests().len(), reached, "{granted}");
    }
}
