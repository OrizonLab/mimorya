//! A person's run calling a Microsoft 365 tool (ADR-117), on the test
//! Graph and provider, and what the model read of its result.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use serde_json::{Value, json};

use super::{Client, TestApi};

/// The payload of the tool's result, as the model read it, the envelope
/// around it removed.
pub async fn tool_result(api: &TestApi, person: &Client<'_>, tool: &str, input: Value) -> Value {
    let seen = Arc::new(Mutex::new(String::new()));
    let observed = seen.clone();
    api.script(vec![
        ScriptedStep::calls("", vec![call(tool, input)]),
        ScriptedStep::dynamic(move |request| {
            if let Some(result) = last_tool_results(request).first() {
                *observed.lock().unwrap() = result.content.clone();
            }
            ScriptedReply::text("Voici ce que contient votre compte.")
        }),
    ]);
    let conversation = person.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = person
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Que dit mon compte Microsoft 365 ?" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.text());
    api.idle().await;
    let content = seen.lock().unwrap().clone();
    let start = content.find('\n').expect("wrapped result") + 1;
    let end = content.rfind('\n').expect("wrapped result");
    serde_json::from_str(&content[start..end]).expect("JSON result")
}

/// The error the tool told the model.
pub fn told(result: &Value) -> String {
    result["erreur"]
        .as_str()
        .unwrap_or_else(|| panic!("an error: {result}"))
        .to_string()
}
