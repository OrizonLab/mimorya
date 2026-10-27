//! What reading an OpenAPI description costs (ADR-114): every part read is
//! paid for by its size, each time it is read, so that a document under the
//! size bound reads quickly whatever shares what: a long `required` list, a
//! long summary or a long enumeration read again through `$ref`, a long
//! pointer. And what the operations offered weigh together is bounded too,
//! however much a small document makes them repeat.

use std::time::{Duration, Instant};

use mimorya_connectors::rest::{Imported, MAX_DOCUMENT_BYTES, MAX_OFFERED_BYTES, import_openapi};

/// Well under what the platform's executor would notice, even unoptimised.
const QUICK: Duration = Duration::from_secs(5);

fn timed(document: &str) -> Imported {
    assert!(document.len() <= MAX_DOCUMENT_BYTES, "{}", document.len());
    let started = Instant::now();
    let imported = import_openapi(document).unwrap();
    let took = started.elapsed();
    assert!(took < QUICK, "{took:?} for {} bytes", document.len());
    imported
}

fn post(schema: &str) -> String {
    format!(
        r##"{{"requestBody":{{"content":{{"application/json":{{"schema":{{"$ref":"{schema}"}}}}}}}}}}"##
    )
}

/// `k` paths sharing one path item.
fn shared_paths(k: usize) -> String {
    let paths: Vec<String> = (0..k)
        .map(|i| format!(r##""/ x{i}":{{"$ref":"#/c/P"}}"##))
        .collect();
    paths.join(",")
}

#[test]
fn a_long_required_list_read_again() {
    let names = vec!["\"a\""; 150_000].join(",");
    let op = post("#/c/B");
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"B":{{"required":[{names}]}},"P":{{"post":{op},"put":{op},"patch":{op}}}}},"paths":{{{}}}}}"##,
        shared_paths(6_000)
    );
    let imported = timed(&document);
    assert!(
        imported.notes[0].contains("non lu(s) : un document trop complexe"),
        "{:?}",
        &imported.notes[..2]
    );
}

#[test]
fn a_long_summary_read_again() {
    let text = "a ".repeat(38_000);
    let op = format!(r##"{{"summary":"{text}"}}"##);
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"P":{{"get":{op},"put":{op},"post":{op},"delete":{op},"patch":{op}}}}},"paths":{{{}}}}}"##,
        shared_paths(13_000)
    );
    let imported = timed(&document);
    assert!(
        imported.notes[0].contains("non lu(s)"),
        "{:?}",
        &imported.notes[..2]
    );
}

#[test]
fn a_long_enumeration_read_again() {
    let values = vec!["\"a\""; 140_000].join(",");
    let fields: Vec<String> = (0..100)
        .map(|i| format!(r##""p{i}":{{"$ref":"#/c/E"}}"##))
        .collect();
    let paths: Vec<String> = (0..8)
        .map(|i| format!(r##""/x{i}":{{"post":{}}}"##, post("#/c/B")))
        .collect();
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"E":{{"type":"string","enum":[{values}]}},"B":{{"type":"object","properties":{{{}}}}}}},"paths":{{{}}}}}"##,
        fields.join(","),
        paths.join(",")
    );
    let imported = timed(&document);
    // Fifty values shown of each field, then said to go on.
    let shown = format!("Valeurs : {}, ….", vec!["a"; 50].join(", "));
    assert_eq!(imported.operations.len(), 8);
    for operation in &imported.operations {
        assert_eq!(operation.parameters.len(), 30);
        assert!(operation.parameters.iter().all(|p| p.description == shown));
    }
}

#[test]
fn a_long_pointer() {
    let key = "k".repeat(330_000);
    let fields: Vec<String> = (0..150)
        .map(|i| format!(r##""p{i}":{{"$ref":"#/c/A"}}"##))
        .collect();
    let paths: Vec<String> = (0..12)
        .map(|i| format!(r##""/x{i}":{{"post":{}}}"##, post("#/c/B")))
        .collect();
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"A":{{"$ref":"#/d/{key}"}},"B":{{"type":"object","properties":{{{}}}}}}},"d":{{"{key}":{{"type":"string"}}}},"paths":{{{}}}}}"##,
        fields.join(","),
        paths.join(",")
    );
    let imported = timed(&document);
    // Refused as it is, never followed.
    let notes = imported.notes.join("\n");
    assert!(
        notes.contains("POST /x0 : champ « p0 » laissé de côté : une référence trop longue"),
        "{notes}"
    );
}

#[test]
fn what_the_operations_offered_weigh_together() {
    // A shared parameter whose description fills its bound with characters
    // of four bytes, thirty of them in every operation.
    let description = "𝒜".repeat(300);
    let parameters: Vec<String> = (0..30)
        .map(|i| format!(r##"{{"$ref":"#/c/Q{i}"}}"##))
        .collect();
    let shared: Vec<String> = (0..30)
        .map(|i| format!(r##""Q{i}":{{"name":"q{i}","in":"query","schema":{{"type":"string"}},"description":"{description}"}}"##))
        .collect();
    let op = format!(r##"{{"parameters":[{}]}}"##, parameters.join(","));
    let paths: Vec<String> = (0..300)
        .map(|i| format!(r##""/x{i}":{{"get":{op}}}"##))
        .collect();
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{{}}},"paths":{{{}}}}}"##,
        shared.join(","),
        paths.join(",")
    );
    let imported = timed(&document);
    let weight: usize = imported
        .operations
        .iter()
        .map(|o| serde_json::to_vec(o).unwrap().len())
        .sum();
    assert!(weight <= MAX_OFFERED_BYTES, "{weight}");
    assert!(imported.operations.len() < 200);
    assert!(
        imported.notes[0].contains("au-delà de ce qu’une réponse porte"),
        "{}",
        imported.notes[0]
    );
}

#[test]
fn many_media_types_read_again() {
    // A shared body offering its JSON last, after thirty thousand others.
    let media: Vec<String> = (0..30_000).map(|i| format!(r#""x/y{i}":{{}}"#)).collect();
    let op = r##"{"requestBody":{"$ref":"#/c/R"}}"##;
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"R":{{"content":{{{},"application/json":{{"schema":{{}}}}}}}},"P":{{"post":{op},"put":{op},"patch":{op}}}}},"paths":{{{}}}}}"##,
        media.join(","),
        shared_paths(6_000)
    );
    let imported = timed(&document);
    // Each read of the body pays for every type listed: the budget is spent
    // within the first few paths, and the others are not read.
    let unread: usize = imported.notes[0]
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_default();
    assert!(unread > 5_900, "{}", imported.notes[0]);
}

#[test]
fn a_shared_path_item_holding_many_keys() {
    // Its methods are looked up, its other keys never listed.
    let keys: Vec<String> = (0..40_000).map(|i| format!(r#""x-{i}":0"#)).collect();
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"P":{{{},"get":{{"operationId":"list"}}}}}},"paths":{{{}}}}}"##,
        keys.join(","),
        shared_paths(10_000)
    );
    let imported = timed(&document);
    assert!(imported.operations.is_empty());
    // A thousand operations converted at most: the other paths not read.
    assert!(
        imported.notes[0].starts_with("9000 chemin(s) non lu(s)"),
        "{}",
        imported.notes[0]
    );
}

#[test]
fn long_field_names_read_again() {
    // Two hundred fields of three thousand characters, in a body every
    // operation shares: each read pays for the names it goes through.
    let fields: Vec<String> = (0..200)
        .map(|i| format!(r#""{}{i}":{{"type":"string"}}"#, "n".repeat(3_000)))
        .collect();
    let op = post("#/c/B");
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"B":{{"type":"object","properties":{{{}}}}},"P":{{"post":{op},"put":{op},"patch":{op}}}}},"paths":{{{}}}}}"##,
        fields.join(","),
        shared_paths(2_000)
    );
    let imported = timed(&document);
    let unread: usize = imported.notes[0]
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_default();
    assert!(unread > 1_900, "{}", imported.notes[0]);
}

#[test]
fn long_parameter_names_read_again() {
    // Two hundred parameters of three thousand characters, listed by a path
    // item every path shares: each listing pays for the names.
    let parameters: Vec<String> = (0..200)
        .map(|i| format!(r#"{{"name":"{}{i}","in":"query"}}"#, "n".repeat(3_000)))
        .collect();
    let op = r#"{"operationId":"list"}"#;
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"P":{{"parameters":[{}],"get":{op},"put":{op},"delete":{op}}}}},"paths":{{{}}}}}"##,
        parameters.join(","),
        shared_paths(2_000)
    );
    let imported = timed(&document);
    let unread: usize = imported.notes[0]
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_default();
    assert!(unread > 1_900, "{}", imported.notes[0]);
}

#[test]
fn long_values_of_an_enumeration_read_again() {
    // What costs most for its weight: texts every character of which is
    // looked at. Fifty long values of short words, in an enumeration two
    // hundred parameters of a shared path item point to.
    let words = "a ".repeat(700);
    let values = vec![format!("\"{words}\""); 50].join(",");
    let parameters: Vec<String> = (0..200)
        .map(|i| format!(r##"{{"name":"p{i}","in":"query","schema":{{"$ref":"#/c/S"}}}}"##))
        .collect();
    let op = r#"{"operationId":"list"}"#;
    let document = format!(
        r##"{{"openapi":"3.0.0","c":{{"S":{{"type":"string","enum":[{values}]}},"P":{{"parameters":[{}],"get":{op},"put":{op},"post":{op},"delete":{op},"patch":{op}}}}},"paths":{{{}}}}}"##,
        parameters.join(","),
        shared_paths(2_000)
    );
    let imported = timed(&document);
    let unread: usize = imported.notes[0]
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_default();
    // A text is paid for four times as dear as a name: the budget is spent
    // within the first paths (each of five operations of 200 parameters).
    assert!(unread >= 1_995, "{}", imported.notes[0]);
}
