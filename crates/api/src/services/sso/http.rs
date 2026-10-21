//! How the server reaches an identity provider: the connectors' hardened
//! client (ADR-025: public addresses only unless the operator allows the
//! private network, no redirect, no proxy from the environment), every
//! address checked before connecting, each exchange bounded in time and
//! size, for `openidconnect`.

use std::fmt;
use std::pin::Pin;
use std::time::Duration;

use mimorya_connectors::mcp::{NetworkPolicy, http_client};

use super::address::address_problem;
use openidconnect::{AsyncHttpClient, HttpRequest, HttpResponse, http};
use reqwest_mcp::Client;

const TIMEOUT: Duration = Duration::from_secs(10);
/// Discovery documents and key sets are a few kilobytes.
const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub struct ExchangeError(String);

impl fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExchangeError {}

pub struct ProviderHttp {
    client: Client,
    policy: NetworkPolicy,
}

impl ProviderHttp {
    pub fn new(policy: NetworkPolicy) -> anyhow::Result<Self> {
        Ok(Self {
            client: http_client(policy)?,
            policy,
        })
    }

    async fn bounded(&self, request: HttpRequest) -> Result<HttpResponse, ExchangeError> {
        // Whatever names it: the stored issuer, or the provider's declaration.
        if let Some(problem) = address_problem(&request.uri().to_string(), self.policy) {
            return Err(ExchangeError(problem));
        }
        tokio::time::timeout(TIMEOUT, self.exchange(request))
            .await
            .map_err(|_| ExchangeError("le fournisseur ne répond pas à temps".into()))?
    }

    async fn exchange(&self, request: HttpRequest) -> Result<HttpResponse, ExchangeError> {
        let failed = |error: reqwest_mcp::Error| ExchangeError(error.to_string());
        let request = reqwest_mcp::Request::try_from(request).map_err(failed)?;
        let mut response = self.client.execute(request).await.map_err(failed)?;
        let mut answer = http::Response::builder().status(response.status());
        for (name, value) in response.headers() {
            answer = answer.header(name, value);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(ExchangeError("réponse du fournisseur trop longue".into()));
            }
            body.extend_from_slice(&chunk);
        }
        answer
            .body(body)
            .map_err(|error| ExchangeError(error.to_string()))
    }
}

impl<'c> AsyncHttpClient<'c> for ProviderHttp {
    type Error = ExchangeError;
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, ExchangeError>> + Send + 'c>>;

    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(self.bounded(request))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn never_connects_to_an_address_the_policy_refuses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}/keys", listener.local_addr().unwrap());
        let http = ProviderHttp::new(NetworkPolicy::default()).unwrap();
        let request = http::Request::get(&address).body(Vec::new()).unwrap();
        let refused = http.call(request).await.unwrap_err();
        assert_eq!(refused.to_string(), "http : https:// attendu");
        let request = http::Request::get(address.replacen("http:", "https:", 1))
            .body(Vec::new())
            .unwrap();
        let refused = http.call(request).await.unwrap_err();
        assert!(refused.to_string().contains("réseau privé"), "{refused}");
        // Nobody knocked.
        let accepted = tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
        assert!(accepted.is_err());
    }
}
