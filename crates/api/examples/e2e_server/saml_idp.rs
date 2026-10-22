//! A SAML 2.0 provider for the end-to-end tests (ADR-068), at an address of
//! its own (`SAML_IDP_ADDRESS`, which `saml-sign-on.spec.ts` declares).
//! The tests read its metadata, register the company's application with it
//! as an administrator would (the metadata served at the company's entity
//! ID), then sign a collaborator in through its pages, shown as a provider
//! without scripts shows them: an address typed, then the response posted
//! back to the application by a button.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Form, RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, put};
use serde::Deserialize;

use crate::saml_provider::{MockSamlProvider, SamlPerson};

struct Provider {
    base: String,
    saml: MockSamlProvider,
    /// The metadata of the application registered with it.
    application: Mutex<Option<String>>,
}

type Shared = Arc<Provider>;

/// Listens on `address`; its base address is returned.
pub async fn start_on(address: &str) -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let base = format!("http://{}", listener.local_addr()?);
    let provider = Arc::new(Provider {
        base: base.clone(),
        saml: MockSamlProvider::at(&base),
        application: Mutex::new(None),
    });
    let router = Router::new()
        .route("/metadata", get(metadata))
        .route("/application", put(register))
        .route("/sso", get(visit).post(sign_in))
        .with_state(provider);
    tokio::spawn(async move { axum::serve(listener, router).await });
    Ok(base)
}

async fn metadata(State(provider): State<Shared>) -> Response {
    (
        [(header::CONTENT_TYPE, "application/samlmetadata+xml")],
        provider.saml.metadata(),
    )
        .into_response()
}

/// The application's metadata, as an administrator imports it.
async fn register(State(provider): State<Shared>, metadata: String) -> StatusCode {
    *provider.application.lock().expect("provider") = Some(metadata);
    StatusCode::NO_CONTENT
}

/// The sign-in page the application sends the browser to.
async fn visit(RawQuery(query): RawQuery) -> Response {
    let Some(query) = query else {
        return (StatusCode::BAD_REQUEST, "Demande SAML absente").into_response();
    };
    page(
        "Fournisseur SAML de test",
        &format!(
            r#"<form method="post" action="/sso">
<input type="hidden" name="query" value="{}">
<label>Adresse e-mail <input name="email" type="email" required></label>
<button type="submit">Se connecter</button>
</form>"#,
            escape(&query)
        ),
    )
}

#[derive(Deserialize)]
struct SignIn {
    /// The query the application's address carried.
    query: String,
    email: String,
}

/// Signs in the address typed: the response, posted back by a button.
async fn sign_in(State(provider): State<Shared>, Form(form): Form<SignIn>) -> Response {
    let Some(application) = provider.application.lock().expect("provider").clone() else {
        return (StatusCode::CONFLICT, "Aucune application enregistrée").into_response();
    };
    let address = format!("{}/sso?{}", provider.base, form.query);
    let answer = provider.saml.answer(
        &address,
        &application,
        &application,
        &SamlPerson::named(&form.email),
    );
    let relay_state = answer
        .relay_state
        .map(|state| {
            format!(
                r#"<input type="hidden" name="RelayState" value="{}">"#,
                escape(&state)
            )
        })
        .unwrap_or_default();
    page(
        "Fournisseur SAML de test",
        &format!(
            r#"<p>Connecté en tant que {}.</p>
<form method="post" action="{}">
<input type="hidden" name="SAMLResponse" value="{}">{relay_state}
<button type="submit">Continuer</button>
</form>"#,
            escape(&form.email),
            escape(&answer.entity_endpoint),
            escape(&answer.context)
        ),
    )
}

fn page(title: &str, body: &str) -> Response {
    Html(format!(
        r#"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><link rel="icon" href="data:,"><title>{title}</title></head>
<body><h1>{title}</h1>
{body}
</body></html>"#
    ))
    .into_response()
}

/// Text set in an attribute or an element.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
