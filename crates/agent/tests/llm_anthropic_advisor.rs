//! Anthropic's advisor on the wire (ADR-207, step 3): its server tool after
//! the platform's, under its beta as long as the history holds its blocks;
//! the reasoning bound to a withdrawn advisor dropped, for the models that
//! reason by default; its consultations read from the usage's iterations,
//! its blocks kept to be replayed, and a paused turn read as such; its
//! betas in the request's header (against a local server, never the real
//! API).

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use mimorya_agent::llm::anthropic::{
    ADVISOR_BETA, AnthropicOptions, AnthropicProvider, MessageAccumulator, RequestSettings,
    SERVER_FALLBACK_BETA, SseParser, THINKING_BINDING_BETA, betas, build_body,
};
use mimorya_agent::llm::{
    AdvisorTool, Consultation, LlmMessage, LlmProvider, LlmRequest, LlmToolSpec, LlmUsage,
    NativeContent, StopReason,
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

fn advisor(cache: bool) -> AdvisorTool {
    AdvisorTool {
        model: "claude-opus-5-5".into(),
        max_uses: 5,
        cache,
    }
}

fn search_tool() -> LlmToolSpec {
    LlmToolSpec {
        name: "jobs_search".into(),
        description: "Chercher".into(),
        input_schema: json!({ "type": "object" }),
    }
}

#[test]
fn the_advisor_follows_the_platform_s_tools_under_its_beta() {
    let mut request = LlmRequest::new("Consignes", vec![LlmMessage::user("Bonjour")]);
    request.tools = vec![search_tool()];
    request.advisor = Some(advisor(true));
    let settings = settings("claude-haiku-5-5");
    let body = build_body(&settings, &request);
    assert_eq!(body["tools"][0]["name"], "jobs_search");
    assert_eq!(
        body["tools"][1],
        json!({
            "type": "advisor_20260301",
            "name": "advisor",
            "model": "claude-opus-5-5",
            "max_uses": 5,
            "caching": { "type": "ephemeral", "ttl": "5m" },
        })
    );
    assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
    assert!(body.get("thinking").is_none());
    assert_eq!(betas(&settings, &request), [ADVISOR_BETA]);

    // Alone, without cache, it is still a tool the model may use.
    let mut alone = LlmRequest::new("Consignes", vec![LlmMessage::user("Bonjour")]);
    alone.advisor = Some(advisor(false));
    let body = build_body(&settings, &alone);
    assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    assert!(body["tools"][0].get("caching").is_none());
    assert_eq!(body["tool_choice"], json!({ "type": "auto" }));

    // Without it, nothing of it.
    let plain = LlmRequest::new("Consignes", vec![LlmMessage::user("Bonjour")]);
    assert!(build_body(&settings, &plain).get("tools").is_none());
    assert!(betas(&settings, &plain).is_empty());
}

#[test]
fn its_blocks_in_the_history_keep_its_beta_once_it_is_withdrawn() {
    let consulted = LlmMessage::Assistant {
        content: "Synthèse.".into(),
        tool_calls: Vec::new(),
        native: Some(NativeContent {
            provider: "anthropic".into(),
            content: json!([
                { "type": "server_tool_use", "id": "srvtoolu_1", "name": "advisor", "input": {} },
                { "type": "advisor_tool_result", "tool_use_id": "srvtoolu_1",
                  "content": { "type": "advisor_redacted_result", "encrypted_content": "chiffré" } },
                { "type": "text", "text": "Synthèse." }
            ]),
        }),
    };
    let mut request = LlmRequest::new(
        "Consignes",
        vec![
            LlmMessage::user("Bonjour"),
            consulted,
            LlmMessage::user("Et ensuite ?"),
        ],
    );
    request.drop_unbound_thinking = true;
    let opus = settings("claude-opus-5-5");
    let body = build_body(&opus, &request);
    // Replayed as they came.
    assert_eq!(
        body["messages"][1]["content"][1]["type"],
        "advisor_tool_result"
    );
    // The reasoning bound to it dropped rather than refused.
    assert_eq!(
        body["thinking"],
        json!({ "type": "adaptive", "block_binding": { "prefix_mismatch_behavior": "drop_block" } })
    );
    assert_eq!(
        betas(&opus, &request),
        [SERVER_FALLBACK_BETA, ADVISOR_BETA, THINKING_BINDING_BETA]
    );
    // A model that reasons only when asked has no reasoning to drop.
    let older = settings("claude-opus-4-8");
    assert!(build_body(&older, &request).get("thinking").is_none());
    assert_eq!(betas(&older, &request), [ADVISOR_BETA]);
}

/// A turn whose model consulted its advisor once, then paused while it
/// consults it again.
fn consulted_then_paused() -> String {
    let iterations = json!([
        { "type": "message", "input_tokens": 400, "cache_read_input_tokens": 0,
          "cache_creation_input_tokens": 0, "output_tokens": 80 },
        { "type": "advisor_message", "model": "claude-opus-5-5", "input_tokens": 800,
          "cache_read_input_tokens": 100, "cache_creation_input_tokens": 50, "output_tokens": 1500 },
        { "type": "message", "input_tokens": 1200, "cache_read_input_tokens": 0,
          "cache_creation_input_tokens": 0, "output_tokens": 60 }
    ]);
    let events = [
        json!({ "type": "message_start", "message": { "model": "claude-haiku-5-5", "content": [],
                "usage": { "input_tokens": 400, "output_tokens": 1 } } }),
        json!({ "type": "content_block_start", "index": 0, "content_block":
                { "type": "server_tool_use", "id": "srvtoolu_1", "name": "advisor", "input": {} } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({ "type": "content_block_start", "index": 1, "content_block":
                { "type": "advisor_tool_result", "tool_use_id": "srvtoolu_1", "content":
                  { "type": "advisor_redacted_result", "encrypted_content": "chiffré",
                    "stop_reason": "end_turn" } } }),
        json!({ "type": "content_block_stop", "index": 1 }),
        json!({ "type": "content_block_start", "index": 2, "content_block":
                { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 2, "delta":
                { "type": "text_delta", "text": "Je vérifie." } }),
        json!({ "type": "content_block_stop", "index": 2 }),
        json!({ "type": "content_block_start", "index": 3, "content_block":
                { "type": "server_tool_use", "id": "srvtoolu_2", "name": "advisor", "input": {} } }),
        json!({ "type": "content_block_stop", "index": 3 }),
        json!({ "type": "message_delta", "delta": { "stop_reason": "pause_turn" },
                "usage": { "input_tokens": 1600, "output_tokens": 140, "iterations": iterations } }),
        json!({ "type": "message_stop" }),
    ];
    events
        .iter()
        .map(|data| {
            format!(
                "event: {}\r\ndata: {data}\r\n\r\n",
                data["type"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn its_consultations_are_read_apart_and_its_blocks_kept() {
    let mut parser = SseParser::default();
    let mut message = MessageAccumulator::new("claude-haiku-5-5");
    for event in parser.push(consulted_then_paused().as_bytes()) {
        message.apply(&event, &|_| {}).unwrap();
    }
    let response = message.finish().unwrap();
    assert_eq!(response.stop_reason, StopReason::PauseTurn);
    // Its second call has no result yet: it runs at the next request.
    assert!(response.advisor_pending);
    // The model's own tokens; its advisor's apart.
    assert_eq!(
        (response.usage.input_tokens, response.usage.output_tokens),
        (1600, 140)
    );
    assert_eq!(
        response.consultations,
        [Consultation {
            model: "claude-opus-5-5".into(),
            usage: LlmUsage {
                input_tokens: 800,
                output_tokens: 1500,
                cache_read_tokens: 100,
                cache_write_tokens: 50,
            },
        }]
    );
    // The advice is not the answer's text; its blocks are kept to replay.
    assert_eq!(response.text, "Je vérifie.");
    assert!(response.tool_calls.is_empty());
    let blocks = response.native.unwrap().content;
    let kinds: Vec<&str> = blocks
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "server_tool_use",
            "advisor_tool_result",
            "text",
            "server_tool_use"
        ]
    );
    assert_eq!(blocks[0]["input"], json!({}));
    assert_eq!(blocks[1]["content"]["encrypted_content"], "chiffré");
}

fn transcript(events: &[Value]) -> String {
    events
        .iter()
        .map(|data| {
            format!(
                "event: {}\r\ndata: {data}\r\n\r\n",
                data["type"].as_str().unwrap()
            )
        })
        .collect()
}

fn read(stream: &str) -> mimorya_agent::llm::LlmResponse {
    let mut parser = SseParser::default();
    let mut message = MessageAccumulator::new("claude-haiku-5-5");
    for event in parser.push(stream.as_bytes()) {
        message.apply(&event, &|_| {}).unwrap();
    }
    message.finish().unwrap()
}

#[test]
fn a_consultation_asked_beside_tools_is_pending_one_answered_is_not() {
    let start = json!({ "type": "message_start", "message": { "model": "claude-haiku-5-5",
                        "content": [], "usage": { "input_tokens": 10, "output_tokens": 1 } } });
    let advisor = json!({ "type": "content_block_start", "index": 0, "content_block":
                          { "type": "server_tool_use", "id": "srvtoolu_1", "name": "advisor",
                            "input": {} } });
    let stop = |index: u32| json!({ "type": "content_block_stop", "index": index });
    let end = |reason: &str| {
        json!({ "type": "message_delta", "delta": { "stop_reason": reason },
                                      "usage": { "output_tokens": 5 } })
    };
    // The advisor called beside a tool: the answer stops on the tool, the
    // consultation still to run.
    let beside_tools = read(&transcript(&[
        start.clone(),
        advisor.clone(),
        stop(0),
        json!({ "type": "content_block_start", "index": 1, "content_block":
                { "type": "tool_use", "id": "toolu_1", "name": "jobs_search", "input": {} } }),
        stop(1),
        end("tool_use"),
        json!({ "type": "message_stop" }),
    ]));
    assert_eq!(beside_tools.stop_reason, StopReason::ToolUse);
    assert_eq!(beside_tools.tool_calls.len(), 1);
    assert!(beside_tools.advisor_pending);
    // Consulted and answered.
    let answered = read(&transcript(&[
        start,
        advisor,
        stop(0),
        json!({ "type": "content_block_start", "index": 1, "content_block":
                { "type": "advisor_tool_result", "tool_use_id": "srvtoolu_1", "content":
                  { "type": "advisor_redacted_result", "encrypted_content": "chiffré" } } }),
        stop(1),
        end("end_turn"),
        json!({ "type": "message_stop" }),
    ]));
    assert!(!answered.advisor_pending);
}

async fn serve(body: String) -> (String, Arc<Mutex<Vec<(HeaderMap, Value)>>>) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let state = captured.clone();
    let app = Router::new().route(
        "/v1/messages",
        post(
            move |headers: HeaderMap, axum::Json(sent): axum::Json<Value>| {
                let state = state.clone();
                let body = body.clone();
                async move {
                    state.lock().unwrap().push((headers, sent));
                    (
                        StatusCode::OK,
                        [("content-type", "text/event-stream")],
                        body,
                    )
                        .into_response()
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, captured)
}

#[tokio::test]
async fn its_betas_go_in_the_request_s_header() {
    let (url, captured) = serve(consulted_then_paused()).await;
    let mut request = LlmRequest::new("Consignes", vec![LlmMessage::user("Bonjour")]);
    request.advisor = Some(advisor(false));
    for model in ["claude-opus-5-5", "claude-haiku-5-5"] {
        let provider = AnthropicProvider::new(AnthropicOptions {
            api_key: Some("test-key".into()),
            model: Some(model.into()),
            base_url: Some(url.clone()),
            ..AnthropicOptions::default()
        });
        let response = provider.complete(&request).await.unwrap();
        assert_eq!(response.consultations.len(), 1);
    }
    let sent = captured.lock().unwrap();
    assert_eq!(
        sent[0].0["anthropic-beta"],
        "server-side-fallback-2026-07-01,advisor-tool-2026-03-01"
    );
    assert_eq!(sent[1].0["anthropic-beta"], "advisor-tool-2026-03-01");
    assert_eq!(sent[1].1["tools"][0]["type"], "advisor_20260301");
}
