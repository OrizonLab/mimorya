//! What the agent's document tools answer in a run (ADR-096): the call the
//! model makes, what it is handed back, and the sources its answer shows.

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call, last_tool_results};
use serde_json::Value;
use uuid::Uuid;

use super::{Client, TestApi};

/// What the model, in a run of `client`, is handed back for its call of
/// `tool` with `input`: the content of the result, as the model reads it,
/// and the run's conversation.
pub async fn tool_answer(
    api: &TestApi,
    client: &Client<'_>,
    tool: &str,
    input: Value,
) -> (String, String) {
    api.script(vec![
        ScriptedStep::calls("", vec![call(tool, input)]),
        ScriptedStep::text("Voici ce que disent les documents."),
    ]);
    let conversation = client
        .post("/conversations", serde_json::json!({}))
        .await
        .json();
    let id = conversation["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sent = client
        .post(
            &format!("/conversations/{id}/messages"),
            serde_json::json!({ "content": "Que disent nos documents ?" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    let results = last_tool_results(requests.last().unwrap());
    (results[0].content.clone(), id)
}

/// The data inside a result's envelope.
pub fn payload(content: &str) -> Value {
    assert!(
        content.starts_with("<donnees_outil source=\"document_"),
        "{content}"
    );
    let start = content.find('\n').unwrap() + 1;
    let end = content.rfind('\n').unwrap();
    serde_json::from_str(&content[start..end]).unwrap()
}

/// The passages `client`'s run finds for `query`: their documents and texts.
pub async fn found(api: &TestApi, client: &Client<'_>, query: &str) -> Vec<(Uuid, String)> {
    let input = serde_json::json!({ "query": query, "limit": 10 });
    let (answer, _) = tool_answer(api, client, "document_search", input).await;
    payload(&answer)
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| {
            let id = hit["document"].as_str().unwrap().parse().unwrap();
            (id, hit["texte"].as_str().unwrap().to_string())
        })
        .collect()
}

/// The sources the answer of the conversation `id` shows.
pub async fn sources(client: &Client<'_>, id: &str) -> Vec<Value> {
    let thread = client.get(&format!("/conversations/{id}")).await.json();
    let messages = thread["messages"].as_array().unwrap();
    let answer = messages.last().unwrap();
    assert_eq!(answer["role"], "assistant", "{thread}");
    answer["metadata"]["evidence"].as_array().unwrap().clone()
}

/// The document titled `title` among those `client` reads.
pub async fn titled(client: &Client<'_>, title: &str) -> Uuid {
    let list = client.get("/documents").await.json();
    let document = list["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["title"] == title)
        .unwrap_or_else(|| panic!("{title} not listed: {list}"));
    document["id"].as_str().unwrap().parse().unwrap()
}
