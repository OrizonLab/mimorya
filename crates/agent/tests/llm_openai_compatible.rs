//! OpenAI-compatible adapter against a local server.

use std::sync::{Arc, Mutex};

use axum::routing::post;
use axum::{Json, Router};
use mimorya_agent::llm::openai_compatible::{OpenAiCompatibleOptions, OpenAiCompatibleProvider};
use mimorya_agent::llm::{LlmMessage, LlmProvider, LlmRequest, LlmToolSpec, StopReason};
use serde_json::{Value, json};

#[tokio::test]
async fn maps_requests_and_responses() {
    let received: Arc<Mutex<Option<Value>>> = Arc::default();
    let store = received.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let store = store.clone();
            async move {
                *store.lock().unwrap() = Some(body);
                Json(json!({
                    "model": "mistral-small",
                    "choices": [{
                        "finish_reason": "tool_calls",
                        "message": {
                            "content": "Je cherche.",
                            "tool_calls": [
                                { "id": "c1", "type": "function", "function": { "name": "jobs_search", "arguments": "{\"query\":\"x\"}" } },
                                { "id": "c2", "type": "function", "function": { "name": "jobs_search", "arguments": "{oops" } }
                            ]
                        }
                    }],
                    "usage": { "prompt_tokens": 50, "completion_tokens": 7, "prompt_tokens_details": { "cached_tokens": 10 } }
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleOptions {
        id: "ollama".into(),
        model: "mistral-small".into(),
        api_key: None,
        base_url: Some(url),
    });
    let mut request = LlmRequest::new("Consignes", vec![LlmMessage::user("Bonjour")]);
    request.tools = vec![LlmToolSpec {
        name: "jobs_search".into(),
        description: "Recherche".into(),
        input_schema: json!({ "type": "object" }),
    }];
    let response = provider.complete(&request).await.unwrap();

    let body = received.lock().unwrap().clone().unwrap();
    assert_eq!(
        body["messages"][0],
        json!({ "role": "system", "content": "Consignes" })
    );
    assert_eq!(body["tools"][0]["function"]["name"], "jobs_search");
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(response.text, "Je cherche.");
    assert_eq!(response.tool_calls[0].input, json!({ "query": "x" }));
    // Invalid JSON arguments are passed on for the schema validation to reject.
    assert_eq!(
        response.tool_calls[1].input,
        json!({ "__invalid_json__": "{oops" })
    );
    // `prompt_tokens` includes the cached tokens: they are counted once.
    assert_eq!(response.usage.input_total(), 50);
    assert_eq!(
        (
            response.usage.input_tokens,
            response.usage.cache_read_tokens
        ),
        (40, 10)
    );
    assert_eq!(response.provider, "ollama");
}
