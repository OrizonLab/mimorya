//! Anthropic adapter: request mapping, stream reconstruction and HTTP
//! behaviour (against a local server, never the real API).

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use mimorya_agent::llm::anthropic::{
    AnthropicOptions, AnthropicProvider, MessageAccumulator, RequestSettings, SseParser, build_body,
};
use mimorya_agent::llm::{
    Effort, LlmErrorKind, LlmMessage, LlmProvider, LlmRequest, LlmToolCall, LlmToolResult,
    LlmToolSpec, LlmUsage, NativeContent, PriceTable, StopReason,
};
use serde_json::{Value, json};

fn settings(model: &str) -> RequestSettings {
    RequestSettings {
        model: model.into(),
        max_output_tokens: 64_000,
        server_side_fallback: mimorya_agent::llm::anthropic::supports_server_fallback(model),
        eager_input_streaming: true,
    }
}

fn thinking_text_and_tool_use() -> Value {
    json!([
        { "type": "thinking", "thinking": "", "signature": "sig" },
        { "type": "text", "text": "Je vérifie." },
        { "type": "tool_use", "id": "toolu_1", "name": "jobs_search", "input": { "query": "x" } }
    ])
}

#[test]
fn maps_requests_enables_caching_server_side_fallback_and_effort() {
    let mut request = LlmRequest::new(
        "Consignes",
        vec![
            LlmMessage::user("Bonjour"),
            LlmMessage::Assistant {
                content: "Ancien".into(),
                tool_calls: vec![LlmToolCall {
                    id: "t0".into(),
                    name: "jobs_search".into(),
                    input: json!({}),
                }],
                native: None,
            },
            LlmMessage::ToolResults {
                results: vec![LlmToolResult {
                    tool_call_id: "t0".into(),
                    content: "[]".into(),
                    is_error: true,
                }],
            },
        ],
    );
    request.effort = Some(Effort::High);
    request.tools = vec![LlmToolSpec {
        name: "jobs_search".into(),
        description: "Recherche".into(),
        input_schema: json!({ "type": "object", "properties": {} }),
    }];

    let body = build_body(&settings("claude-opus-5-5"), &request);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["system"], "Consignes");
    assert_eq!(body["cache_control"], json!({ "type": "ephemeral" }));
    assert_eq!(body["output_config"], json!({ "effort": "high" }));
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
    assert_eq!(body["stream"], true);
    assert_eq!(
        body["tools"],
        json!([{
            "name": "jobs_search",
            "description": "Recherche",
            "input_schema": { "type": "object", "properties": {} },
            "eager_input_streaming": true
        }])
    );
    assert_eq!(
        body["messages"],
        json!([
            { "role": "user", "content": "Bonjour" },
            { "role": "assistant", "content": [
                { "type": "text", "text": "Ancien" },
                { "type": "tool_use", "id": "t0", "name": "jobs_search", "input": {} }
            ] },
            { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "t0", "content": "[]", "is_error": true }
            ] }
        ])
    );
}

#[test]
fn replays_assistant_turns_verbatim_reasoning_blocks_included() {
    let request = LlmRequest::new(
        "s",
        vec![
            LlmMessage::user("Q"),
            LlmMessage::Assistant {
                content: "Je vérifie.".into(),
                tool_calls: vec![],
                native: Some(NativeContent {
                    provider: "anthropic".into(),
                    content: thinking_text_and_tool_use(),
                }),
            },
            LlmMessage::user("Suite"),
        ],
    );
    let body = build_body(&settings("claude-opus-5-5"), &request);
    assert_eq!(body["messages"][1]["content"], thinking_text_and_tool_use());
    assert!(body.get("tools").is_none());
}

#[test]
fn does_not_request_fallbacks_on_models_that_do_not_support_them() {
    let body = build_body(
        &settings("claude-haiku-5-5"),
        &LlmRequest::new("s", vec![LlmMessage::user("Q")]),
    );
    assert!(body.get("fallbacks").is_none());
}

/// A streamed reply: thinking, text in two deltas, a tool call in two JSON parts.
fn transcript() -> String {
    let events = [
        (
            "message_start",
            json!({ "type": "message_start", "message": { "model": "claude-opus-5-5", "content": [], "usage": { "input_tokens": 1000, "output_tokens": 1, "cache_read_input_tokens": 5000, "cache_creation_input_tokens": 0 } } }),
        ),
        (
            "content_block_start",
            json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "", "signature": "" } }),
        ),
        (
            "content_block_delta",
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "sig" } }),
        ),
        (
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 }),
        ),
        (
            "content_block_start",
            json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }),
        ),
        (
            "content_block_delta",
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "Je " } }),
        ),
        ("ping", json!({ "type": "ping" })),
        (
            "content_block_delta",
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "vérifie." } }),
        ),
        (
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 1 }),
        ),
        (
            "content_block_start",
            json!({ "type": "content_block_start", "index": 2, "content_block": { "type": "tool_use", "id": "toolu_1", "name": "jobs_search", "input": {} } }),
        ),
        (
            "content_block_delta",
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": "{\"query\":" } }),
        ),
        (
            "content_block_delta",
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": " \"x\"}" } }),
        ),
        (
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 2 }),
        ),
        (
            "message_delta",
            json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" }, "usage": { "output_tokens": 200 } }),
        ),
        ("message_stop", json!({ "type": "message_stop" })),
    ];
    events
        .iter()
        .map(|(name, data)| format!("event: {name}\r\ndata: {data}\r\n\r\n"))
        .collect()
}

#[test]
fn rebuilds_the_message_from_the_stream_whatever_the_chunking() {
    let bytes = transcript().into_bytes();
    for chunk_size in [1, 7, 64, bytes.len()] {
        let deltas = Mutex::new(Vec::new());
        let mut parser = SseParser::default();
        let mut message = MessageAccumulator::new("claude-opus-5-5");
        for chunk in bytes.chunks(chunk_size) {
            for event in parser.push(chunk) {
                message
                    .apply(&event, &|t: &str| {
                        deltas.lock().unwrap().push(t.to_string())
                    })
                    .unwrap();
            }
        }
        let response = message.finish().unwrap();
        assert_eq!(response.text, "Je vérifie.");
        assert_eq!(
            response.tool_calls,
            vec![LlmToolCall {
                id: "toolu_1".into(),
                name: "jobs_search".into(),
                input: json!({ "query": "x" })
            }]
        );
        assert_eq!(response.stop_reason, StopReason::ToolUse);
        assert_eq!(
            response.usage,
            LlmUsage {
                input_tokens: 1000,
                output_tokens: 200,
                cache_read_tokens: 5000,
                cache_write_tokens: 0
            }
        );
        assert_eq!(
            response.native.unwrap().content,
            thinking_text_and_tool_use()
        );
        assert_eq!(*deltas.lock().unwrap(), vec!["Je ", "vérifie."]);
        // 1000×4 + 200×20 + 5000×0.2 micro-dollars
        assert_eq!(
            PriceTable::default().cost_micros(&response.model, &response.usage),
            Some(9000)
        );
    }
}

#[test]
fn reports_a_stream_cut_before_its_end_as_a_retryable_failure() {
    let truncated: String = transcript()
        .split("event: message_stop")
        .next()
        .unwrap()
        .to_string();
    let mut parser = SseParser::default();
    let mut message = MessageAccumulator::new("claude-opus-5-5");
    for event in parser.push(truncated.as_bytes()) {
        message.apply(&event, &|_| {}).unwrap();
    }
    let error = message.finish().unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Network);
    assert!(error.retryable());
}

#[derive(Clone, Default)]
struct Captured {
    headers: Arc<Mutex<Vec<HeaderMap>>>,
}

async fn serve(status: StatusCode, body: String) -> (String, Captured) {
    let captured = Captured::default();
    let state = captured.clone();
    let app = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let state = state.clone();
            let body = body.clone();
            async move {
                state.headers.lock().unwrap().push(headers);
                (status, [("content-type", "text/event-stream")], body).into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, captured)
}

fn provider(url: &str, model: &str) -> AnthropicProvider {
    AnthropicProvider::new(AnthropicOptions {
        api_key: Some("test-key".into()),
        model: Some(model.into()),
        base_url: Some(url.into()),
        ..AnthropicOptions::default()
    })
}

#[tokio::test]
async fn streams_over_http_with_the_expected_headers() {
    let (url, captured) = serve(StatusCode::OK, transcript()).await;
    let deltas = Arc::new(Mutex::new(Vec::new()));
    let sink = deltas.clone();
    let mut request = LlmRequest::new("s", vec![LlmMessage::user("Q")]);
    request.on_text_delta = Some(Arc::new(move |t: &str| {
        sink.lock().unwrap().push(t.to_string())
    }));

    let response = provider(&url, "claude-opus-5-5")
        .complete(&request)
        .await
        .unwrap();
    assert_eq!(response.text, "Je vérifie.");
    assert_eq!(*deltas.lock().unwrap(), vec!["Je ", "vérifie."]);
    let headers = captured.headers.lock().unwrap()[0].clone();
    assert_eq!(headers["x-api-key"], "test-key");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(headers["anthropic-beta"], "server-side-fallback-2026-07-01");

    provider(&url, "claude-haiku-5-5")
        .complete(&request)
        .await
        .unwrap();
    assert!(
        captured.headers.lock().unwrap()[1]
            .get("anthropic-beta")
            .is_none()
    );
}

#[tokio::test]
async fn maps_http_errors_to_the_neutral_taxonomy() {
    let request = LlmRequest::new("s", vec![LlmMessage::user("Q")]);
    let cases = [
        (StatusCode::TOO_MANY_REQUESTS, "{}", LlmErrorKind::RateLimit),
        (StatusCode::UNAUTHORIZED, "{}", LlmErrorKind::Auth),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"prompt is too long: 250000 tokens"}}"#,
            LlmErrorKind::ContextOverflow,
        ),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"bad field"}}"#,
            LlmErrorKind::InvalidRequest,
        ),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "{}",
            LlmErrorKind::Overloaded,
        ),
    ];
    for (status, body, kind) in cases {
        let (url, _) = serve(status, body.into()).await;
        let error = provider(&url, "claude-opus-5-5")
            .complete(&request)
            .await
            .unwrap_err();
        assert_eq!(error.kind, kind, "{status}");
    }
    let unreachable = provider("http://127.0.0.1:9", "claude-opus-5-5")
        .complete(&request)
        .await
        .unwrap_err();
    assert_eq!(unreachable.kind, LlmErrorKind::Network);
}
