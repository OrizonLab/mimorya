//! What a REST API's declaration may hold (ADR-112): names, texts, paths
//! and parameters bounded, visible and consistent; and a request that never
//! leaves the path its administrators wrote, whatever a parameter brings.

use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::rest::{
    MAX_CONFIG_BYTES, MAX_OPERATIONS, RestConfig, RestOperation, build, config_problem,
    header_problem, parse_input, path_placeholders,
};
use serde_json::{Value, json};

const BASE: &str = "https://api.example.com/v1";
const PUBLIC: NetworkPolicy = NetworkPolicy {
    allow_private_network: false,
    allow_stdio: false,
};

fn operation(changes: Value) -> Value {
    let mut operation = json!({
        "name": "get_ticket",
        "title": "Lire un ticket",
        "description": "Renvoie un ticket\net son état.",
        "method": "GET",
        "path": "/tickets/{id}",
        "parameters": [{ "name": "id", "in": "path", "type": "string", "required": true }],
    });
    for (key, value) in changes.as_object().unwrap() {
        operation[key] = value.clone();
    }
    operation
}

fn config(operations: Value) -> RestConfig {
    serde_json::from_value(json!({ "prefix": "crm", "url": BASE, "operations": operations }))
        .unwrap()
}

fn problem(changes: Value) -> Option<String> {
    config_problem(&config(json!([operation(changes)])), PUBLIC)
}

#[test]
fn accepts_a_well_formed_declaration() {
    assert_eq!(problem(json!({})), None);
    let post = json!({
        "name": "create_ticket", "method": "POST", "path": "/sites/{site}/tickets",
        "parameters": [
            { "name": "site", "in": "path", "type": "integer", "required": true },
            { "name": "dry-run", "in": "query", "type": "boolean", "description": "Essai." },
            { "name": "title", "in": "body", "type": "string", "required": true },
            { "name": "cost.max", "in": "body", "type": "number" },
        ],
    });
    assert_eq!(problem(post), None);
    let mut keyed = config(json!([operation(json!({}))]));
    keyed.token_header = Some("X-Api-Key".into());
    assert_eq!(config_problem(&keyed, PUBLIC), None);
}

#[test]
fn refuses_what_an_operation_may_not_hold() {
    let path_id = json!([{ "name": "id", "in": "path", "type": "string", "required": true }]);
    for (changes, expected) in [
        (json!({ "name": "Get" }), "Nom d’opération invalide « Get »"),
        (
            json!({ "name": "a".repeat(41) }),
            "Nom d’opération invalide",
        ),
        (json!({ "title": "" }), "Opération get_ticket : intitulé"),
        (
            json!({ "title": "Deux\nlignes" }),
            "Opération get_ticket : intitulé",
        ),
        (
            json!({ "description": " " }),
            "Opération get_ticket : description",
        ),
        (
            json!({ "description": "Caché\u{200b}" }),
            "Opération get_ticket : description",
        ),
        (
            json!({ "description": "é".repeat(1001) }),
            "Opération get_ticket : description",
        ),
        (
            json!({ "path": "tickets" }),
            "chemin de 1 à 500 caractères commençant par /",
        ),
        (
            json!({ "path": "/tickets/{id}?x=1" }),
            "caractère « ? » refusé",
        ),
        (
            json!({ "path": "/tickets/{id}#x" }),
            "caractère « # » refusé",
        ),
        (
            json!({ "path": "/tickets/%2e%2e/{id}" }),
            "caractère « % » refusé",
        ),
        (
            json!({ "path": "/tickets/../{id}" }),
            "segment . ou .. refusé",
        ),
        (json!({ "path": "/tickets/{id" }), "accolade ouverte"),
        (
            json!({ "path": "/tickets/{i d}" }),
            "{i d} : nom de paramètre invalide",
        ),
        (
            json!({ "path": "/tickets/é{id}" }),
            "caractère « é » refusé",
        ),
        (
            json!({ "path": "/tickets/{id}/{other}" }),
            "{other} figure dans le chemin",
        ),
        (
            json!({ "path": "/tickets" }),
            "le chemin ne contient pas {id}",
        ),
        (
            json!({ "parameters": [{ "name": "id", "in": "path", "type": "string" }] }),
            "le paramètre de chemin « id » est toujours requis",
        ),
        (
            json!({ "parameters": [path_id[0], { "name": "q", "in": "body", "type": "string" }] }),
            "une requête GET n’a pas de corps",
        ),
        (
            json!({ "parameters": [path_id[0], path_id[0]] }),
            "paramètre « id » déclaré deux fois",
        ),
        (
            json!({ "parameters": [path_id[0], { "name": "1q", "in": "query", "type": "string" }] }),
            "nom de paramètre invalide « 1q »",
        ),
        (
            json!({ "parameters": [path_id[0], {
                "name": "q", "in": "query", "type": "string", "description": "Deux\nlignes",
            }] }),
            "description du paramètre « q »",
        ),
    ] {
        let found = problem(changes.clone()).unwrap_or_default();
        assert!(found.contains(expected), "{changes} : {found}");
    }
    let many: Vec<Value> = (0..31)
        .map(|i| json!({ "name": format!("q{i}"), "in": "query", "type": "string" }))
        .collect();
    let mut parameters = vec![path_id[0].clone()];
    parameters.extend(many);
    let found = problem(json!({ "parameters": parameters })).unwrap();
    assert!(found.contains("30 paramètres au plus"), "{found}");
}

#[test]
fn refuses_what_an_api_may_not_hold() {
    let one = json!([operation(json!({}))]);
    assert_eq!(
        config_problem(&config(json!([])), PUBLIC).as_deref(),
        Some("Une opération au moins")
    );
    let twice = json!([operation(json!({})), operation(json!({}))]);
    let found = config_problem(&config(twice), PUBLIC).unwrap();
    assert_eq!(found, "Opération « get_ticket » déclarée deux fois");
    let all: Vec<Value> = (0..=MAX_OPERATIONS)
        .map(|i| operation(json!({ "name": format!("op{i}") })))
        .collect();
    let found = config_problem(&config(json!(all)), PUBLIC).unwrap();
    assert_eq!(found, "50 opérations au plus");
    // Long descriptions, each allowed, too long together.
    let described: Vec<Value> = (0..10)
        .map(|i| {
            json!({ "name": format!("q{i}"), "in": "query", "type": "string",
            "description": "é".repeat(300) })
        })
        .chain([json!({ "name": "id", "in": "path", "type": "string", "required": true })])
        .collect();
    let long: Vec<Value> = (0..MAX_OPERATIONS)
        .map(|i| {
            operation(
                json!({ "name": format!("op{i}"), "description": "é".repeat(1000),
            "parameters": described }),
            )
        })
        .collect();
    assert!(serde_json::to_vec(&long).unwrap().len() > MAX_CONFIG_BYTES);
    let found = config_problem(&config(json!(long)), PUBLIC).unwrap();
    assert!(found.starts_with("Déclaration trop longue"), "{found}");

    let mut refused = config(one.clone());
    refused.prefix = "CRM".into();
    assert!(
        config_problem(&refused, PUBLIC)
            .unwrap()
            .starts_with("Préfixe invalide")
    );
    for url in [
        "http://api.example.com",
        "https://10.0.0.1/v1",
        "https://api.example.com/v1?key=x",
    ] {
        let mut refused = config(one.clone());
        refused.url = url.into();
        assert!(config_problem(&refused, PUBLIC).is_some(), "{url}");
    }
    assert_eq!(header_problem("X-Api-Key"), None);
    for header in [
        "Authorization",
        "cookie",
        "Proxy-Authorization",
        "Sec-Fetch",
        "Content-Range",
        "content-disposition",
        "User-Agent",
        "Accept-Encoding",
        "X Key",
        "1x",
        "Hôte",
    ] {
        assert!(header_problem(header).is_some(), "{header}");
    }
}

fn operation_with_path(path: &str) -> Value {
    operation(json!({
        "path": path,
        "parameters": [{ "name": "a", "in": "path", "type": "string", "required": true }],
    }))
}

#[test]
fn keeps_every_request_under_the_path_written() {
    let operation: RestOperation = serde_json::from_value(operation(json!({
        "path": "/sites/{a}{b}/x",
        "parameters": [
            { "name": "a", "in": "path", "type": "string", "required": true },
            { "name": "b", "in": "path", "type": "string", "required": true },
            { "name": "q", "in": "query", "type": "string" },
        ],
    })))
    .unwrap();
    assert_eq!(path_placeholders(&operation.path).unwrap(), ["a", "b"]);
    let built = |input: Value| {
        let input = parse_input(&operation, &input).unwrap();
        build(BASE, &operation, &input)
    };
    let request = built(json!({ "a": "1/2", "b": "?q=x#f", "q": "a&b=c" })).unwrap();
    assert_eq!(
        request.url.as_str(),
        "https://api.example.com/v1/sites/1%2F2%3Fq%3Dx%23f/x?q=a%26b%3Dc"
    );
    assert_eq!(request.body, None);
    // Together they would climb a segment (and alone, `.` is refused as an input).
    // Values each refused alone or not, the path they make is checked as
    // sent, as a server decoding `%2F` reads it, and decoded once more.
    for climbing in [
        json!({ "a": ".", "b": "." }),
        json!({ "a": "x/", "b": "." }),
        json!({ "a": "x%2F", "b": "%2E" }),
    ] {
        assert_eq!(
            build(BASE, &operation, &climbing).unwrap_err(),
            "le chemin obtenu contient un segment . ou ..",
            "{climbing}"
        );
    }
    // Next to what the administrators wrote as well.
    let dotted: RestOperation =
        serde_json::from_value(operation_with_path("/files/{a}..")).unwrap();
    assert!(build(BASE, &dotted, &json!({ "a": "x/" })).is_err());
    assert!(build(BASE, &dotted, &json!({ "a": "x" })).is_ok());
    // A trailing slash on the address changes nothing.
    let input = parse_input(&operation, &json!({ "a": "s", "b": "1" })).unwrap();
    let request = build("https://api.example.com/v1/", &operation, &input).unwrap();
    assert_eq!(
        request.url.as_str(),
        "https://api.example.com/v1/sites/s1/x"
    );
}
