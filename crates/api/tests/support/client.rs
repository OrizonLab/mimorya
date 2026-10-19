//! Requests sent with a session cookie, and the responses they get.

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use serde_json::Value;

use super::TestApi;

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| {
            panic!(
                "JSON body expected, got {:?}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn error_message(&self) -> String {
        self.json()["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

pub struct Client<'a> {
    api: &'a TestApi,
    pub session: String,
}

impl<'a> Client<'a> {
    pub fn new(api: &'a TestApi, session: &str) -> Self {
        Self {
            api,
            session: session.to_string(),
        }
    }

    pub fn api(&self) -> &'a TestApi {
        self.api
    }

    fn request(&self, method: Method, url: &str) -> axum::http::request::Builder {
        let builder = Request::builder()
            .method(method.clone())
            .uri(url)
            .header("cookie", format!("mimorya_session={}", self.session));
        if method == Method::GET {
            builder
        } else {
            builder.header("x-mimorya-request", "1")
        }
    }

    pub async fn get(&self, url: &str) -> TestResponse {
        let request = self.request(Method::GET, url).body(Body::empty()).unwrap();
        self.api.send(request).await
    }

    pub async fn get_with(&self, url: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = self.request(Method::GET, url);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.api.send(builder.body(Body::empty()).unwrap()).await
    }

    pub async fn post(&self, url: &str, payload: Value) -> TestResponse {
        self.send_json(Method::POST, url, payload).await
    }

    pub async fn patch(&self, url: &str, payload: Value) -> TestResponse {
        self.send_json(Method::PATCH, url, payload).await
    }

    pub async fn patch_with(
        &self,
        url: &str,
        payload: Value,
        headers: &[(&str, &str)],
    ) -> TestResponse {
        self.send_json_with(Method::PATCH, url, payload, headers)
            .await
    }

    pub async fn put(&self, url: &str, payload: Value) -> TestResponse {
        self.send_json(Method::PUT, url, payload).await
    }

    async fn send_json(&self, method: Method, url: &str, payload: Value) -> TestResponse {
        self.send_json_with(method, url, payload, &[]).await
    }

    async fn send_json_with(
        &self,
        method: Method,
        url: &str,
        payload: Value,
        headers: &[(&str, &str)],
    ) -> TestResponse {
        let mut builder = self
            .request(method, url)
            .header("content-type", "application/json");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(Body::from(payload.to_string())).unwrap();
        self.api.send(request).await
    }

    pub async fn delete(&self, url: &str) -> TestResponse {
        let request = self
            .request(Method::DELETE, url)
            .body(Body::empty())
            .unwrap();
        self.api.send(request).await
    }
}
