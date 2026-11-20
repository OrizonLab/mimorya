//! TypeSafe's System One endpoint as Jev answers it, on a local port, for
//! tests (feature `testing`, ADR-213): only `JEV_KEY` as a bearer opens it;
//! each question answered of its type (yes at `JEV_YES`, the first option
//! of a choice, the lowest level of a score or the one a test sets), what
//! it was asked noted
//! without the key; and, on demand, overloaded, answering what is not
//! System One, or never answering.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Map, Value, json};

/// The only key the simulated Jev accepts.
pub const JEV_KEY: &str = "ts-cle-simulee-jev-0123456789abcdef";
/// The probability of yes it answers to each yes-or-no question.
pub const JEV_YES: f64 = 0.97;

/// How the simulated Jev answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JevBehaviour {
    #[default]
    Answers,
    /// 529, as TypeSafe overloaded.
    Overloaded,
    /// A 200 that is not System One.
    Unreadable,
    /// Nothing, for a minute.
    Silent,
}

/// What it was asked: whether the right key came, and the body.
#[derive(Debug, Clone)]
pub struct JevAsked {
    pub keyed: bool,
    pub body: Value,
}

#[derive(Default)]
struct Inner {
    behaviour: Mutex<JevBehaviour>,
    asked: Mutex<Vec<JevAsked>>,
    /// The level a score is answered at, the lowest without one.
    score_level: Mutex<usize>,
}

/// The simulated Jev, served until the process ends.
#[derive(Clone)]
pub struct FakeJev {
    /// Its base address, `http://127.0.0.1:<port>`.
    pub url: String,
    inner: Arc<Inner>,
}

impl FakeJev {
    pub async fn serve() -> Self {
        let inner = Arc::new(Inner::default());
        let router = Router::new()
            .route("/v1/systemone", post(system_one))
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("simulated Jev");
        });
        Self {
            url: format!("http://{address}"),
            inner,
        }
    }

    pub fn behave(&self, behaviour: JevBehaviour) {
        *self.inner.behaviour.lock().expect("behaviour") = behaviour;
    }

    /// Answers each score at `level` from now on (the highest there is,
    /// if beyond).
    pub fn score_at(&self, level: usize) {
        *self.inner.score_level.lock().expect("score level") = level;
    }

    pub fn asked(&self) -> Vec<JevAsked> {
        self.inner.asked.lock().expect("asked").clone()
    }
}

async fn system_one(
    State(inner): State<Arc<Inner>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let keyed = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(&format!("Bearer {JEV_KEY}"));
    inner.asked.lock().expect("asked").push(JevAsked {
        keyed,
        body: body.clone(),
    });
    if !keyed {
        return refused(StatusCode::UNAUTHORIZED, "authentication_error");
    }
    let behaviour = *inner.behaviour.lock().expect("behaviour");
    match behaviour {
        JevBehaviour::Answers => {
            let level = *inner.score_level.lock().expect("score level");
            axum::Json(answered(&body, level)).into_response()
        }
        JevBehaviour::Overloaded => refused(
            StatusCode::from_u16(529).unwrap_or(StatusCode::SERVICE_UNAVAILABLE),
            "overloaded_error",
        ),
        JevBehaviour::Unreadable => axum::Json(json!({ "ok": true })).into_response(),
        JevBehaviour::Silent => {
            tokio::time::sleep(Duration::from_secs(60)).await;
            StatusCode::GATEWAY_TIMEOUT.into_response()
        }
    }
}

fn refused(status: StatusCode, kind: &str) -> Response {
    let body = json!({ "type": "error", "error": { "type": kind, "message": kind } });
    (status, axum::Json(body)).into_response()
}

/// Each question answered of its type.
fn answered(body: &Value, level: usize) -> Value {
    let empty = Map::new();
    let questions = body["questions"].as_object().unwrap_or(&empty);
    let answers: Map<String, Value> = questions
        .iter()
        .map(|(name, question)| (name.clone(), answer(question, level)))
        .collect();
    json!({
        "model": body["model"],
        "answers": answers,
        "usage": { "input_tokens": 100 + 10 * questions.len(), "output_tokens": 0 },
    })
}

fn answer(question: &Value, level: usize) -> Value {
    match question["type"].as_str() {
        Some("choice") => {
            let options: Vec<&String> = question["criteria"]
                .as_object()
                .map(|criteria| criteria.keys().collect())
                .unwrap_or_default();
            let probabilities: Map<String, Value> = options
                .iter()
                .enumerate()
                .map(|(at, option)| {
                    let p = if at == 0 {
                        0.9
                    } else {
                        0.1 / (options.len() - 1) as f64
                    };
                    ((*option).clone(), json!(p))
                })
                .collect();
            json!({
                "type": "choice",
                "choice": options.first(),
                "confidence": 0.9,
                "probabilities": probabilities,
            })
        }
        Some("score") => {
            let levels = question["criteria"].as_array().map_or(1, Vec::len).max(1);
            let said = level.min(levels - 1);
            let probabilities: Map<String, Value> = (0..levels)
                .map(|at| {
                    let p = if at == said {
                        0.9
                    } else {
                        0.1 / (levels - 1) as f64
                    };
                    (at.to_string(), json!(p))
                })
                .collect();
            json!({
                "type": "score",
                "score": said as f64,
                "confidence": 0.9,
                "probabilities": probabilities,
            })
        }
        _ => json!({ "type": "noul", "noul": JEV_YES }),
    }
}
