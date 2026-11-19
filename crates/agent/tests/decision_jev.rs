//! Jev as an engine of decisions (ADR-213), against a local server, never
//! TypeSafe's: the pinned version asked at `/v1/systemone`, the key as a
//! bearer and nowhere else; its answers read; each failure — its key or the
//! request refused, an overload, an unreadable body — read as such, which
//! the service falls back from.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use mimorya_agent::decision::{
    DecisionEngine, DecisionQuestion, DecisionRequest, DecisionState, EngineFailure, JevEngine,
};
use serde_json::{Value, json};

const KEY: &str = "ts-cle-de-test-jev-0123456789";

type Seen = Arc<Mutex<Vec<(HeaderMap, Value)>>>;

/// A System One endpoint answering `status` and `body`, noting what it saw.
async fn serve(status: StatusCode, body: String) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let noted = seen.clone();
    let app = Router::new().route(
        "/v1/systemone",
        post(
            move |headers: HeaderMap, axum::Json(asked): axum::Json<Value>| {
                let noted = noted.clone();
                let body = body.clone();
                async move {
                    noted.lock().unwrap().push((headers, asked));
                    (status, [("content-type", "application/json")], body).into_response()
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}

fn request() -> DecisionRequest {
    DecisionRequest {
        usage: "test",
        state: DecisionState::new().field("message", "Bonjour", 100),
        questions: vec![DecisionQuestion::noul(
            "salutation",
            "Ce message est-il une salutation ?",
        )],
    }
}

fn answered() -> String {
    json!({
        "model": "jev-1.14.0",
        "answers": { "salutation": { "type": "noul", "noul": 0.98 } },
        "usage": { "input_tokens": 120, "output_tokens": 12 }
    })
    .to_string()
}

#[tokio::test]
async fn the_pinned_version_is_asked_with_the_key_as_a_bearer_alone() {
    let (url, seen) = serve(StatusCode::OK, answered()).await;
    let jev = JevEngine::new(KEY.into(), "jev-1.14.0", &url);
    let answer = jev.answer(&request()).await.unwrap();
    assert_eq!(
        (answer.provider.as_str(), answer.model.as_str()),
        ("typesafe", "jev-1.14.0")
    );
    assert_eq!(answer.usage.input_tokens, 120);
    let (headers, asked) = seen.lock().unwrap()[0].clone();
    assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(asked["model"], "jev-1.14.0");
    assert_eq!(asked["state"], json!({ "message": "Bonjour" }));
    assert_eq!(asked["questions"]["salutation"]["type"], "noul");
    // The key goes nowhere else.
    assert!(!asked.to_string().contains(KEY));
}

#[tokio::test]
async fn each_failure_is_read_as_such() {
    for (status, body, failure) in [
        (
            StatusCode::UNAUTHORIZED,
            "{}".to_string(),
            EngineFailure::Refused,
        ),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}".to_string(),
            EngineFailure::Refused,
        ),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "{}".to_string(),
            EngineFailure::Refused,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            "{}".to_string(),
            EngineFailure::Unavailable,
        ),
        (
            StatusCode::from_u16(529).unwrap(),
            "{}".to_string(),
            EngineFailure::Unavailable,
        ),
        (
            StatusCode::OK,
            "pas du JSON".to_string(),
            EngineFailure::Invalid,
        ),
        (
            StatusCode::OK,
            json!({ "detail": "inattendu" }).to_string(),
            EngineFailure::Invalid,
        ),
    ] {
        let (url, _) = serve(status, body).await;
        let jev = JevEngine::new(KEY.into(), "jev-1.14.0", &url);
        assert_eq!(jev.answer(&request()).await, Err(failure), "{status}");
    }
    // Unreachable.
    let jev = JevEngine::new(KEY.into(), "jev-1.14.0", "http://127.0.0.1:9");
    assert_eq!(
        jev.answer(&request()).await,
        Err(EngineFailure::Unavailable)
    );
}
