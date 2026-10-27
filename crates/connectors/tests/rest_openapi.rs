//! The operations an OpenAPI 3 description offers (ADR-114): each one a
//! declaration would accept, named, titled and described from the document,
//! its parameters of a simple type; what cannot be declared left aside and
//! said; a description that cannot be read, and an address the description
//! does not complete, said too.

use mimorya_connectors::rest::{
    HttpMethod, ParameterLocation, ParameterType, RestOperation, import_openapi,
};
use serde_json::{Value, json};

fn import(document: &Value) -> mimorya_connectors::rest::Imported {
    import_openapi(&document.to_string()).unwrap()
}

fn find<'a>(operations: &'a [RestOperation], name: &str) -> &'a RestOperation {
    operations
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("{name} : {operations:#?}"))
}

/// A help desk's API: what most descriptions hold.
fn help_desk() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": { "title": "Assistance", "version": "1" },
        "servers": [{
            "url": "https://{region}.desk.example.com/v2/",
            "variables": { "region": { "default": "eu" } },
        }],
        "paths": {
            "/tickets/{ticketId}": {
                "parameters": [{
                    "name": "ticketId", "in": "path", "required": true,
                    "schema": { "type": "string" }, "description": "Identifiant\ndu ticket.",
                }],
                "get": {
                    "operationId": "getTicketByID",
                    "summary": "Lire un ticket",
                    "description": "Renvoie un ticket\u{200b} et son état.",
                    "parameters": [
                        { "$ref": "#/components/parameters/Fields" },
                        {
                            "name": "ticketId", "in": "path", "required": true,
                            "schema": { "type": "string" }, "description": "Son numéro.",
                        },
                        { "name": "X-Trace", "in": "header", "schema": { "type": "string" } },
                        { "name": "tags", "in": "query", "schema": { "type": "array" } },
                    ],
                },
                "patch": {
                    "operationId": "updateTicket",
                    "requestBody": {
                        "required": true,
                        "content": { "application/json; charset=utf-8": {
                            "schema": { "$ref": "#/components/schemas/TicketChange" },
                        } },
                    },
                },
                "delete": { "operationId": "deleteTicket", "deprecated": true },
                "head": { "operationId": "ticketExists" },
            },
            "/tickets": {
                "get": {
                    "summary": "Lister les tickets",
                    "requestBody": { "content": { "application/json": { "schema": {} } } },
                    "parameters": [{
                        "name": "state", "in": "query",
                        "schema": { "type": ["string", "null"], "enum": ["ouvert", "clos"] },
                    }],
                },
                "post": {
                    "operationId": "getTicketByID",
                    "summary": "Créer un ticket",
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": {
                            "type": "object",
                            "required": ["title", "site"],
                            "properties": {
                                "title": { "type": "string" },
                                "site": { "type": "integer" },
                                "notes": { "type": "array", "items": { "type": "string" } },
                            },
                        } } },
                    },
                },
            },
            "/notes": {
                "put": {
                    "operationId": "putNote",
                    "requestBody": { "content": { "application/json": { "schema": {
                        "type": "object",
                        "required": ["text"],
                        "properties": { "text": { "type": "string" } },
                    } } } },
                },
            },
            "/sessions": {
                "post": {
                    "operationId": "signIn",
                    "parameters": [{
                        "name": "X-Tenant", "in": "header", "required": true,
                        "schema": { "type": "string" },
                    }],
                },
            },
            "/exports/{format}": {
                "get": { "operationId": "export", "parameters": [] },
            },
            "/files/%2e%2e/{id}": {
                "get": {
                    "operationId": "climb",
                    "parameters": [{
                        "name": "id", "in": "path", "required": true,
                        "schema": { "type": "string" },
                    }],
                },
            },
            "/uploads": {
                "post": {
                    "operationId": "upload",
                    "requestBody": {
                        "required": true,
                        "content": { "multipart/form-data": { "schema": { "type": "object" } } },
                    },
                },
            },
        },
        "components": {
            "parameters": {
                "Fields": {
                    "name": "fields", "in": "query", "schema": { "type": "string" },
                    "description": "Champs renvoyés.",
                },
            },
            "schemas": {
                "TicketChange": {
                    "allOf": [
                        { "$ref": "#/components/schemas/Stamped" },
                        {
                            "type": "object",
                            "required": ["state"],
                            "properties": {
                                "state": { "type": "string", "enum": ["ouvert", "clos"] },
                                "priority": { "type": "integer", "description": "De 1 à 5." },
                                "assignee": { "type": "object" },
                                "ticketId": { "type": "string" },
                            },
                        },
                    ],
                },
                "Stamped": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "readOnly": true },
                        "updatedAt": { "type": "string" },
                    },
                },
            },
        },
    })
}

#[test]
fn offers_what_a_declaration_would_accept() {
    let imported = import(&help_desk());
    assert_eq!(
        imported.url.as_deref(),
        Some("https://eu.desk.example.com/v2")
    );
    let names: Vec<&str> = imported
        .operations
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "get_ticket_by_id",
            "update_ticket",
            "get_tickets",
            "get_ticket_by_id_2",
            "put_note"
        ]
    );

    let read = find(&imported.operations, "get_ticket_by_id");
    assert_eq!(read.title, "Lire un ticket");
    // A hidden character is taken out, and said.
    assert_eq!(read.description, "Renvoie un ticket et son état.");
    assert_eq!(read.path, "/tickets/{ticketId}");
    let parameters: Vec<(&str, ParameterLocation, bool)> = read
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.location, p.required))
        .collect();
    assert_eq!(
        parameters,
        [
            ("ticketId", ParameterLocation::Path, true),
            ("fields", ParameterLocation::Query, false),
        ]
    );
    // The operation's own definition, where the path item's stood.
    assert_eq!(read.parameters[0].description, "Son numéro.");
    assert_eq!(
        find(&imported.operations, "update_ticket").parameters[0].description,
        "Identifiant du ticket."
    );

    let change = find(&imported.operations, "update_ticket");
    assert_eq!(change.method, HttpMethod::Patch);
    // No summary: the request names it.
    assert_eq!(change.title, "updateTicket");
    assert_eq!(change.description, "PATCH /tickets/{ticketId}");
    let body: Vec<(&str, ParameterType, bool)> = change
        .parameters
        .iter()
        .filter(|p| p.location == ParameterLocation::Body)
        .map(|p| (p.name.as_str(), p.kind, p.required))
        .collect();
    // `allOf` merged, a read-only field left out.
    assert_eq!(
        body,
        [
            ("updatedAt", ParameterType::String, false),
            ("state", ParameterType::String, true),
            ("priority", ParameterType::Integer, false),
        ]
    );
    let state = change
        .parameters
        .iter()
        .find(|p| p.name == "state")
        .unwrap();
    assert_eq!(state.description, "Valeurs : ouvert, clos.");

    let list = find(&imported.operations, "get_tickets");
    assert_eq!(list.title, "Lister les tickets");
    assert_eq!(list.parameters[0].kind, ParameterType::String);

    // An optional body requires none of its fields.
    let note = find(&imported.operations, "put_note");
    assert!(!note.parameters[0].required);

    let create = find(&imported.operations, "get_ticket_by_id_2");
    assert_eq!(create.method, HttpMethod::Post);
    assert_eq!(create.parameters.len(), 2);

    let notes = imported.notes.join("\n");
    for expected in [
        "GET /tickets/{ticketId} : paramètre « X-Trace » laissé de côté : un en-tête",
        "GET /tickets/{ticketId} : paramètre « tags » laissé de côté : un tableau",
        "GET /tickets/{ticketId} : caractères invisibles retirés de ses textes",
        "PATCH /tickets/{ticketId} : paramètre « assignee » laissé de côté : un objet",
        "PATCH /tickets/{ticketId} : paramètre « ticketId » laissé de côté : un nom déjà pris",
        "GET /tickets : corps facultatif laissé de côté : une requête GET n’en a pas",
        "DELETE /tickets/{ticketId} : laissée de côté, une opération dépréciée",
        "HEAD /tickets/{ticketId} : méthode non prise en charge",
        "POST /tickets : paramètre « notes » laissé de côté : un tableau",
        "POST /sessions : laissée de côté, paramètre requis « X-Tenant » : un en-tête, non pris \
         en charge",
        "GET /exports/{format} : laissée de côté. Opération export : {format} figure dans le \
         chemin",
        "GET /files/%2e%2e/{id} : laissée de côté. Opération climb : caractère « % » refusé",
        "POST /uploads : laissée de côté, son corps requis : un contenu autre que JSON",
    ] {
        assert!(notes.contains(expected), "{expected}\n---\n{notes}");
    }
}

#[test]
fn says_what_it_cannot_read() {
    for (document, expected) in [
        ("openapi: 3.0.0", "un document JSON est attendu"),
        (r#"{ "swagger": "2.0", "paths": {} }"#, "Swagger 2.0"),
        (r#"{ "openapi": "2.1", "paths": {} }"#, "OpenAPI 3 attendue"),
        (r#"{ "openapi": "3.1.0" }"#, "aucun chemin"),
    ] {
        let found = import_openapi(document).unwrap_err();
        assert!(found.contains(expected), "{document} : {found}");
    }
    let long = format!(
        "{{\"openapi\":\"3.0.0\",\"x\":\"{}\"}}",
        "a".repeat(1024 * 1024)
    );
    assert!(
        import_openapi(&long)
            .unwrap_err()
            .starts_with("Description trop longue")
    );
}

#[test]
fn takes_no_address_it_cannot_complete() {
    let with = |servers: Value| {
        let imported = import(&json!({ "openapi": "3.1.0", "servers": servers, "paths": {} }));
        (imported.url, imported.notes.join("\n"))
    };
    for (servers, expected) in [
        (json!([]), "n’en donne pas"),
        (
            json!([{ "url": "/v1" }]),
            "« /v1 » n’est pas une adresse complète",
        ),
        (
            json!([{ "url": "https://{host}/v1" }]),
            "variable « host » sans valeur",
        ),
        (
            json!([{ "url": "https://api.example.com/{v" }]),
            "accolade ouverte",
        ),
        (
            json!([{ "url": "https://api.\u{202e}example.com" }]),
            "n’est pas une adresse",
        ),
    ] {
        let (url, notes) = with(servers.clone());
        assert_eq!(url, None, "{servers}");
        assert!(notes.contains(expected), "{servers} : {notes}");
    }
    let (url, _) = with(json!([{ "url": "http://intranet.example.com/api/" }]));
    assert_eq!(url.as_deref(), Some("http://intranet.example.com/api"));
}
