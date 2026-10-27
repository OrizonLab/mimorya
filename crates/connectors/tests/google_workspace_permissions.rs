//! The permissions each Google Workspace tool needs of the person's link
//! (ADR-122, ADR-123): reading Gmail, reading the calendar, sending,
//! writing the calendar's events. A link that does not grant one is
//! refused before Google, and the person is told to link their account
//! again; each other tool works.

mod support;

use std::sync::Arc;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::google_workspace::GOOGLE_WORKSPACE_TOOLS;
use mimorya_connectors::testing::FakeGoogle;
use mimorya_core::account_links::GOOGLE_WORKSPACE;
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, ToolResult};
use serde_json::{Value, json};
use support::google_workspace::google_writing_tools;
use support::linked_accounts::{call, mailbox_executor, validated_call};
use uuid::Uuid;

const SCOPES: &str = "https://www.googleapis.com/auth/";

async fn granting(google: &FakeGoogle, granted: &str) -> ToolExecutor {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account_granting(GOOGLE_WORKSPACE, "acces-lea-1", granted);
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let tools = google_writing_tools(google).await;
    mailbox_executor(tools, user::LEA, &ports, accounts)
}

/// Each tool, called as a run would: the writing ones validated.
async fn used(executor: &ToolExecutor, tool: &str) -> ToolResult {
    let input: Value = match tool {
        "google_gmail_search" => json!({}),
        "google_gmail_read" => json!({ "id": "m1" }),
        "google_calendar_events" => json!({ "from": "2026-10-26", "to": "2026-10-30" }),
        "google_gmail_send" => {
            json!({ "to": ["paul@exemple.fr"], "subject": "Point", "body": "Bonjour." })
        }
        _ => json!({ "subject": "Point", "start": "2026-11-03T09:30:00", "durationMinutes": 30 }),
    };
    match tool {
        "google_gmail_send" | "google_calendar_event_create" => {
            validated_call(executor, tool, input).await
        }
        _ => call(executor, tool, input).await,
    }
}

#[tokio::test]
async fn each_tool_needs_its_own_permission() {
    // What a link grants, and which of the five tools it serves.
    let links = [
        (
            format!("openid email {SCOPES}gmail.readonly"),
            [true, true, false, false, false],
        ),
        (
            format!("{SCOPES}calendar.readonly {SCOPES}gmail.send"),
            [false, false, true, true, false],
        ),
        (
            format!("{SCOPES}gmail.readonly {SCOPES}calendar.events"),
            [true, true, false, false, true],
        ),
    ];
    for (granted, served) in links {
        let google = FakeGoogle::start().await;
        let executor = granting(&google, &granted).await;
        for (tool, serves) in GOOGLE_WORKSPACE_TOOLS.into_iter().zip(served) {
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
        // Only what the link serves reached Google: a search reads the
        // headers of each e-mail it finds.
        let reached: usize = GOOGLE_WORKSPACE_TOOLS
            .into_iter()
            .zip(served)
            .filter(|(_, serves)| *serves)
            .map(|(tool, _)| if tool == "google_gmail_search" { 4 } else { 1 })
            .sum();
        assert_eq!(google.requests().len(), reached, "{granted}");
    }
}
