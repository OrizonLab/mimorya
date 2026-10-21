//! The provider's own sign-in page, served to the end-to-end tests'
//! browser when nobody was chosen nor hinted at (the company's sign-in
//! link suggests no address, ADR-067): it asks who signs in, and their
//! name, then goes on with the same request.

use std::collections::HashMap;

use axum::response::{Html, IntoResponse, Response};

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn sign_in_page(params: &HashMap<String, String>) -> Response {
    let hidden: String = params
        .iter()
        .map(|(name, value)| {
            format!(
                r#"<input type="hidden" name="{}" value="{}">"#,
                escaped(name),
                escaped(value)
            )
        })
        .collect();
    Html(format!(
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8">
<title>Fournisseur d’identité de test</title></head><body>
<form method="get" action="/authorize">{hidden}
<label>Adresse <input name="login_hint" type="email" required></label>
<label>Nom <input name="name"></label>
<button type="submit">Se connecter au fournisseur</button>
</form></body></html>"#
    ))
    .into_response()
}
