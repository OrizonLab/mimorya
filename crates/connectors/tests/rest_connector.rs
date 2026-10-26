//! Calling a REST API a company described (ADR-112), as a run would: each
//! operation is a tool, conservative until mapped; what it sends follows
//! the declaration (path parameters encoded whole, query, JSON body, the
//! token where the API expects it); what the model reads is the status and
//! a bounded body, the token masked even where the bound cuts it.

mod support;

use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::outbound::{MASK, NetworkPolicy, TOO_SLOW, UNREACHABLE};
use mimorya_connectors::rest::MAX_RESPONSE_BYTES;
use mimorya_connectors::testing::{KEY_HEADER, serve_mute, serve_rest_http};
use mimorya_core::autonomy::ToolRisk;
use serde_json::{Value, json};
use support::mcp::{PRIVATE, call_as_reader};
use support::rest::{TOKEN, api, echo, start_rest, tool};

const CALL: Duration = Duration::from_secs(10);

/// The test API's operations: one reading, one creating, one reaching any
/// path of the test API.
fn operations() -> Value {
    json!([
        echo(
            "get_item",
            "GET",
            "/echo/items/{id}",
            json!([
                { "name": "id", "in": "path", "type": "string", "required": true },
                { "name": "page", "in": "query", "type": "integer" },
                { "name": "q", "in": "query", "type": "string" },
            ])
        ),
        echo(
            "create_item",
            "POST",
            "/echo/items",
            json!([
                { "name": "title", "in": "body", "type": "string", "required": true },
                { "name": "urgent", "in": "body", "type": "boolean" },
            ])
        ),
        echo(
            "reach",
            "GET",
            "/{where}",
            json!([
                { "name": "where", "in": "path", "type": "string", "required": true },
            ])
        ),
    ])
}

async fn started(config: Value, secret: Option<&str>) -> ConnectorRuntime {
    start_rest(config, secret, PRIVATE, CALL).await.unwrap()
}

/// The test API's path `path` through the `reach` operation, whose
/// parameter is encoded whole: only paths without `/` reach.
async fn reach(runtime: &ConnectorRuntime, path: &str) -> (Value, bool, String) {
    let result = call_as_reader(runtime, "rest_crm_reach", json!({ "where": path })).await;
    (
        result.content,
        result.is_error,
        result.summary.unwrap_or_default(),
    )
}

#[tokio::test]
async fn sends_what_the_operation_declares_with_the_token() {
    let url = serve_rest_http().await;
    let runtime = started(api(&url, operations()), Some(TOKEN)).await;
    let info = tool(&runtime, "rest_crm_get_item").info().clone();
    assert_eq!(info.risk, ToolRisk::Write);
    assert_eq!(info.required_permissions, ["connector:use"]);
    assert_eq!(
        info.description,
        "[API REST « crm »] Renvoie la requête reçue."
    );

    let input = json!({ "id": "a/b ?#%..", "page": 2, "q": "été & co" });
    let result = call_as_reader(&runtime, "rest_crm_get_item", input).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["statut"], 200);
    let received = &result.content["corps"];
    assert_eq!(received["method"], "GET");
    // Encoded whole: the parameter never adds a segment nor a query.
    assert_eq!(received["path"], "/api/echo/items/a%2Fb%20%3F%23%25..");
    assert_eq!(received["query"], "page=2&q=%C3%A9t%C3%A9+%26+co");
    // Sent as a bearer token, masked in what the API sends back.
    assert_eq!(received["authorization"], format!("Bearer {MASK}"));
    assert_eq!(
        result.summary.as_deref(),
        Some("Réponse de l’API « crm » (HTTP 200)")
    );

    let input = json!({ "title": "Pompe P2", "urgent": true });
    let result = call_as_reader(&runtime, "rest_crm_create_item", input).await;
    let received = &result.content["corps"];
    assert_eq!(received["method"], "POST");
    assert_eq!(received["path"], "/api/echo/items");
    assert_eq!(received["query"], Value::Null);
    assert_eq!(received["contentType"], "application/json");
    assert_eq!(
        received["body"],
        json!({ "title": "Pompe P2", "urgent": true })
    );
}

#[tokio::test]
async fn sends_the_token_in_the_header_the_api_expects() {
    let url = serve_rest_http().await;
    let mut config = api(&url, operations());
    config["tokenHeader"] = json!(KEY_HEADER);
    let runtime = started(config, Some(TOKEN)).await;
    let result = call_as_reader(&runtime, "rest_crm_get_item", json!({ "id": "1" })).await;
    assert_eq!(result.content["corps"]["key"], MASK);
    assert_eq!(result.content["corps"]["authorization"], Value::Null);
    // Without a token, no header at all.
    let runtime = started(api(&url, operations()), None).await;
    let result = call_as_reader(&runtime, "rest_crm_get_item", json!({ "id": "1" })).await;
    assert_eq!(result.content["corps"]["authorization"], Value::Null);
}

#[tokio::test]
async fn refuses_an_input_the_operation_does_not_declare() {
    let url = serve_rest_http().await;
    let runtime = started(api(&url, operations()), None).await;
    let get = tool(&runtime, "rest_crm_get_item");
    let issues = get
        .parse_input(&json!({ "id": 3, "page": 1.5, "q": true, "x": 1 }))
        .unwrap_err();
    assert_eq!(
        issues,
        [
            "x : paramètre inconnu de cette opération",
            "id : texte attendu",
            "page : nombre entier attendu",
            "q : texte attendu",
        ]
    );
    assert_eq!(get.parse_input(&json!({})).unwrap_err(), ["id : requis"]);
    let issues = get.parse_input(&json!({ "id": "" })).unwrap_err();
    assert_eq!(issues, ["id : texte non vide attendu"]);
    // Encoded whole, `/` included, yet a server decoding `%2F` or `%5C`
    // before resolving the path would climb it.
    for climbing in [
        ".",
        "..",
        "../x",
        "a/../../admin",
        "..\\x",
        "x/.",
        "%2e%2e/x",
        "a%2F%2E%2E",
        "%2e",
    ] {
        let issues = get.parse_input(&json!({ "id": climbing })).unwrap_err();
        assert_eq!(
            issues,
            ["id : texte sans segment . ou .. (entre deux / ou \\), qui ferait remonter le chemin"],
            "{climbing}"
        );
    }
    for kept in ["a/b", "v1.2", "...", "..x", "x..", "100%"] {
        assert!(get.parse_input(&json!({ "id": kept })).is_ok(), "{kept}");
    }
    assert!(get.parse_input(&json!([])).is_err());
    let schema = get.input_schema();
    assert_eq!(schema["required"], json!(["id"]));
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["page"]["type"], "integer");
}

#[tokio::test]
async fn shows_whoever_validates_it_what_it_sends() {
    let url = serve_rest_http().await;
    let runtime = started(api(&url, operations()), Some(TOKEN)).await;
    let create = tool(&runtime, "rest_crm_create_item");
    let input = create.parse_input(&json!({ "title": "Pompe" })).unwrap();
    assert_eq!(
        create.describe_action(&input).unwrap(),
        format!(
            "Appeler « Opération create_item » (API REST « crm ») : POST {url}/echo/items avec \
             le corps {{\"title\":\"Pompe\"}}"
        )
    );
    let get = tool(&runtime, "rest_crm_get_item");
    let input = get.parse_input(&json!({ "id": "x y", "page": 3 })).unwrap();
    let described = get.describe_action(&input).unwrap();
    assert!(
        described.ends_with(&format!("GET {url}/echo/items/x%20y?page=3")),
        "{described}"
    );
    assert!(!described.contains(TOKEN));
}

#[tokio::test]
async fn tells_errors_redirects_and_unread_bodies_apart() {
    let url = serve_rest_http().await;
    let runtime = started(api(&url, operations()), None).await;
    let (content, failed, summary) = reach(&runtime, "plain").await;
    assert_eq!(
        (content["corps"].as_str(), failed),
        (Some("Bonjour."), false)
    );
    assert_eq!(summary, "Réponse de l’API « crm » (HTTP 200)");

    let (content, failed, summary) = reach(&runtime, "binary").await;
    assert_eq!(content["corps"], "[contenu image/png de 7 octets, non lu]");
    assert!(!failed, "{summary}");

    // Not followed: the platform never goes where an answer sends it.
    let (content, failed, summary) = reach(&runtime, "redirect").await;
    assert_eq!((content["statut"].as_u64(), failed), (Some(302), true));
    assert_eq!(
        summary,
        "Redirection non suivie par la plateforme (HTTP 302)"
    );

    let runtime = started(
        api(
            &url,
            json!([echo("status", "GET", "/status/404", json!([]))]),
        ),
        None,
    )
    .await;
    let result = call_as_reader(&runtime, "rest_crm_status", json!({})).await;
    assert!(result.is_error);
    assert_eq!(result.content["statut"], 404);
    assert_eq!(result.content["corps"]["erreur"], "ressource introuvable");
    assert_eq!(
        result.summary.as_deref(),
        Some("Erreur de l’API « crm » (HTTP 404)")
    );
}

#[tokio::test]
async fn masks_the_token_an_api_quotes_back() {
    let url = serve_rest_http().await;
    let mut config = api(&url, operations());
    config["tokenHeader"] = json!(KEY_HEADER);
    let runtime = started(config, Some(TOKEN)).await;
    let (content, failed, summary) = reach(&runtime, "token").await;
    assert!(failed);
    assert_eq!(content["corps"]["erreur"], format!("jeton refusé : {MASK}"));
    assert_eq!(summary, "Accès refusé par l’API « crm » (HTTP 401)");
    assert!(!content.to_string().contains(TOKEN));
}

#[tokio::test]
async fn masks_a_token_the_bound_cuts() {
    let url = serve_rest_http().await;
    let mut operations = operations();
    // The token written whole at the start, then again where the reading
    // stops: four of its characters are read, after the bound.
    let at = MAX_RESPONSE_BYTES + TOKEN.len() - 4;
    operations[2] = echo(
        "reach",
        "GET",
        "/cut-token",
        json!([
            { "name": "at", "in": "query", "type": "integer", "required": true },
        ]),
    );
    let mut config = api(&url, operations);
    config["tokenHeader"] = json!(KEY_HEADER);
    let runtime = started(config, Some(TOKEN)).await;
    let result = call_as_reader(&runtime, "rest_crm_reach", json!({ "at": at })).await;
    let body = result.content["corps"].as_str().unwrap();
    assert!(body.starts_with(MASK), "{}", &body[..40]);
    assert!(
        body.chars().all(|c| c == '*' || c == 'x'),
        "{}",
        &body[body.len() - 40..]
    );
    assert!(body.len() <= MAX_RESPONSE_BYTES);
    assert_eq!(result.content["tronque"], "corps coupé après 256 Kio");
}

#[tokio::test]
async fn gives_up_on_an_api_that_does_not_answer() {
    let url = serve_rest_http().await;
    let runtime = start_rest(
        api(&url, operations()),
        None,
        PRIVATE,
        Duration::from_millis(300),
    )
    .await
    .unwrap();
    let (content, failed, _) = reach(&runtime, "slow").await;
    assert!(failed);
    assert_eq!(content["erreur"], format!("{TOO_SLOW} (délai de 0,3 s)"));

    let mute = serve_mute().await.replace("/mcp", "/api");
    let runtime = start_rest(
        api(&mute, operations()),
        None,
        PRIVATE,
        Duration::from_millis(300),
    )
    .await
    .unwrap();
    let (content, _, _) = reach(&runtime, "plain").await;
    assert!(
        content["erreur"].as_str().unwrap().starts_with(TOO_SLOW),
        "{content}"
    );

    let closed = "http://127.0.0.1:9/api";
    let runtime = started(api(closed, operations()), None).await;
    let (content, failed, _) = reach(&runtime, "plain").await;
    assert!(failed);
    assert!(
        content["erreur"].as_str().unwrap().starts_with(UNREACHABLE),
        "{content}"
    );
}

#[tokio::test]
async fn never_reaches_a_private_address_unless_the_operator_allows_it() {
    let url = serve_rest_http().await;
    // Written as an address, refused at start.
    let refused = start_rest(
        api(&url, operations()),
        None,
        NetworkPolicy::default(),
        CALL,
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(refused.to_string(), "Adresse en https:// attendue");
    // Named, refused when called.
    let named = "https://localhost:1/api";
    let runtime = start_rest(
        api(named, operations()),
        None,
        NetworkPolicy::default(),
        CALL,
    )
    .await
    .unwrap();
    let (content, failed, _) = reach(&runtime, "plain").await;
    assert!(failed);
    let message = content["erreur"].as_str().unwrap();
    assert!(
        message.starts_with("Adresse refusée : elle ne mène pas à un serveur public"),
        "{message}"
    );
}

#[tokio::test]
async fn refuses_to_start_a_declaration_it_cannot_use() {
    let url = serve_rest_http().await;
    let start = |config: Value, secret: Option<&'static str>| async move {
        start_rest(config, secret, PRIVATE, CALL)
            .await
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    };
    let mut config = api(&url, operations());
    config["tokenHeader"] = json!("Cookie");
    assert!(start(config, None).await.starts_with("En-tête réservé"));
    let message = start(api(&url, operations()), Some("jeton avec espace")).await;
    assert!(
        message.starts_with("Le jeton enregistré n’a plus un format accepté"),
        "{message}"
    );
    let mut config = api(&url, operations());
    config["extra"] = json!(1);
    let message = start(config, None).await;
    assert!(
        message.starts_with("Déclaration de l’API illisible"),
        "{message}"
    );
    assert_eq!(
        start(api(&url, json!([])), None).await,
        "Une opération au moins"
    );
}
