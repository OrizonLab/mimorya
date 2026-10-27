//! An OpenAPI description read within bounds (ADR-114), however it is
//! written: the parameters an operation may declare, the operations
//! offered, the notes kept, references in a loop, chained or nested too
//! deep, or out of the document, and a small document whose references
//! would take long to read in full.

use std::time::{Duration, Instant};

use mimorya_connectors::rest::{
    MAX_IMPORTED_OPERATIONS, MAX_NOTES, ParameterLocation, import_openapi,
};
use serde_json::{Value, json};

fn import(document: &Value) -> mimorya_connectors::rest::Imported {
    import_openapi(&document.to_string()).unwrap()
}

#[test]
fn keeps_within_what_an_operation_may_declare() {
    let mut properties = serde_json::Map::new();
    for i in 0..40 {
        properties.insert(format!("f{i}"), json!({ "type": "string" }));
    }
    let wide = |required: Vec<String>| {
        json!({
            "openapi": "3.0.0",
            "paths": { "/items": { "post": {
                "operationId": "createItem",
                "summary": "é".repeat(200),
                "description": "d".repeat(2000),
                "requestBody": { "required": true, "content": { "application/json": {
                    "schema": { "type": "object", "required": required, "properties": properties },
                } } },
            } } },
        })
    };
    let imported = import(&wide(vec!["f39".into()]));
    let create = &imported.operations[0];
    assert_eq!(create.parameters.len(), 30);
    assert!(
        create
            .parameters
            .iter()
            .any(|p| p.name == "f39" && p.required)
    );
    assert_eq!(create.title.chars().count(), 120);
    assert!(create.title.ends_with('…'));
    assert_eq!(create.description.chars().count(), 1000);
    let notes = imported.notes.join("\n");
    assert!(
        notes.contains("POST /items : 10 paramètres facultatifs laissés de côté"),
        "{notes}"
    );

    let all = (0..40).map(|i| format!("f{i}")).collect();
    let refused = import(&wide(all));
    assert!(refused.operations.is_empty());
    let notes = refused.notes.join("\n");
    assert!(
        notes.contains("laissée de côté, 40 paramètres requis, 30 au plus"),
        "{notes}"
    );
}

#[test]
fn reads_a_description_within_bounds() {
    // Many operations: the first ones offered, the others counted; notes
    // bounded, those about the whole description first.
    let mut paths = serde_json::Map::new();
    for i in 0..(MAX_IMPORTED_OPERATIONS + 150) {
        paths.insert(
            format!("/items{i}"),
            json!({
                "get": { "operationId": "list" },
                "options": {},
            }),
        );
    }
    let imported = import(&json!({ "openapi": "3.0.0", "paths": paths }));
    assert_eq!(imported.operations.len(), MAX_IMPORTED_OPERATIONS);
    assert_eq!(imported.operations[1].name, "list_2");
    assert!(imported.notes[0].starts_with(
        "150 opération(s) au-delà de ce qu’une réponse porte (200 opérations, 2 Mio)"
    ));
    assert_eq!(imported.notes.len(), MAX_NOTES + 2);
    assert!(imported.notes.last().unwrap().starts_with("… et "));

    // References in a loop, or out of the document.
    let looped = import(&json!({
        "openapi": "3.0.0",
        "paths": {
            "/a": { "post": { "operationId": "loop", "requestBody": {
                "required": true,
                "content": { "application/json": { "schema": { "$ref": "#/components/schemas/A" } } },
            } } },
            "/b": { "get": { "operationId": "away", "parameters": [
                { "$ref": "https://example.com/params.json#/Id" },
            ] } },
        },
        "components": { "schemas": { "A": { "$ref": "#/components/schemas/B" }, "B": { "$ref": "#/components/schemas/A" } } },
    }));
    assert!(looped.operations.is_empty());
    let notes = looped.notes.join("\n");
    assert!(notes.contains("des références en boucle"), "{notes}");
    assert!(notes.contains("une référence hors du document"), "{notes}");

    // Each `allOf` part pointing to a schema made of many parts again: a
    // small document that would take long to read in full.
    let mut schemas = serde_json::Map::new();
    let parts = |next: &str| -> Value {
        Value::Array(
            (0..50)
                .map(|_| json!({ "$ref": format!("#/components/schemas/{next}") }))
                .collect(),
        )
    };
    for (name, next) in [("S0", "S1"), ("S1", "S2"), ("S2", "S3"), ("S3", "S4")] {
        schemas.insert(name.into(), json!({ "allOf": parts(next) }));
    }
    schemas.insert(
        "S4".into(),
        json!({ "type": "object", "properties": { "a": { "type": "string" } } }),
    );
    let mut paths = serde_json::Map::new();
    for i in 0..50 {
        paths.insert(
            format!("/heavy{i}"),
            json!({ "post": { "operationId": format!("heavy{i}"), "requestBody": {
                "content": { "application/json": { "schema": { "$ref": "#/components/schemas/S0" } } },
            } } }),
        );
    }
    let started = Instant::now();
    let heavy = import(&json!({
        "openapi": "3.0.0",
        "paths": paths,
        "components": { "schemas": schemas },
    }));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    // The budget spent on the first one, nothing more is read: not even its
    // texts. The other paths are counted.
    assert!(heavy.operations.is_empty());
    assert!(heavy.notes[0].starts_with("49 chemin(s) non lu(s) : un document trop complexe"));
    let notes = heavy.notes.join("\n");
    assert!(
        notes.contains("POST /heavy0 : laissée de côté, un document trop complexe"),
        "{notes}"
    );
}

#[test]
fn stops_references_and_parts_nested_too_deep() {
    // A chain of references longer than ten, without a loop, and parts
    // nested deeper than four: each stops there rather than read on.
    let mut schemas = serde_json::Map::new();
    for i in 0..11 {
        let next = format!("#/components/schemas/R{}", i + 1);
        schemas.insert(format!("R{i}"), json!({ "$ref": next }));
    }
    for i in 0..5 {
        let next = format!("#/components/schemas/N{}", i + 1);
        schemas.insert(format!("N{i}"), json!({ "allOf": [{ "$ref": next }] }));
    }
    let leaf = json!({ "type": "object", "properties": { "a": { "type": "string" } } });
    schemas.insert("R11".into(), leaf.clone());
    schemas.insert("N5".into(), leaf);
    let post = |id: &str, schema: &str| {
        let schema = json!({ "$ref": format!("#/components/schemas/{schema}") });
        json!({ "post": { "operationId": id, "requestBody": {
            "required": true,
            "content": { "application/json": { "schema": schema } },
        } } })
    };
    let imported = import(&json!({
        "openapi": "3.0.0",
        "paths": {
            "/chained": post("chained", "R0"),
            "/nested": post("nested", "N0"),
            "/shallow": post("shallow", "N2"),
        },
        "components": { "schemas": schemas },
    }));
    let names: Vec<&str> = imported
        .operations
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(names, ["shallow"]);
    assert_eq!(imported.operations[0].parameters[0].name, "a");
    let notes = imported.notes.join("\n");
    for expected in [
        "POST /chained : laissée de côté, son corps requis : des références en boucle ou trop \
         enchaînées",
        "POST /nested : laissée de côté, son corps requis : des allOf trop imbriqués",
    ] {
        assert!(notes.contains(expected), "{expected}\n---\n{notes}");
    }
}

#[test]
fn keeps_a_field_of_a_required_body() {
    // Thirty optional query parameters before a required body of optional
    // fields: one of its fields is kept, else its tool would send no body.
    let query: Vec<Value> = (0..30)
        .map(|i| json!({ "name": format!("q{i}"), "in": "query", "schema": { "type": "string" } }))
        .collect();
    let imported = import(&json!({
        "openapi": "3.0.0",
        "paths": { "/items": { "patch": {
            "operationId": "patchItems",
            "parameters": query,
            "requestBody": { "required": true, "content": { "application/json": { "schema": {
                "properties": { "a": { "type": "string" }, "b": { "type": "string" } },
            } } } },
        } } },
    }));
    let patch = &imported.operations[0];
    assert_eq!(patch.parameters.len(), 30);
    let body: Vec<&str> = patch
        .parameters
        .iter()
        .filter(|p| p.location == ParameterLocation::Body)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(body, ["a"]);
    let notes = imported.notes.join("\n");
    assert!(
        notes.contains("PATCH /items : 2 paramètres facultatifs laissés de côté"),
        "{notes}"
    );
}
