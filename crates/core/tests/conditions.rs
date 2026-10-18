//! ABAC condition language: operators, combinators and JSON validation.

use mimorya_core::authz::{Condition, evaluate_condition};
use serde_json::{Value, json};

fn env() -> Value {
    json!({
        "subject": { "userId": "u1", "orgUnitIds": ["t1", "t2"], "clearanceRank": 1, "attributes": {} },
        "resource": { "type": "doc", "sensitivityRank": 2, "attributes": { "status": "open", "tags": ["a"] } },
        "context": { "actorType": "agent" },
    })
}

fn cond(value: Value) -> Condition {
    serde_json::from_value(value).expect("valid condition")
}

#[test]
fn supports_every_operator() {
    let cases = [
        (
            json!({ "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": "doc" } }),
            true,
        ),
        (
            json!({ "op": "neq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "closed" } }),
            true,
        ),
        (
            json!({ "op": "in", "left": { "value": "t2" }, "right": { "attr": "subject.orgUnitIds" } }),
            true,
        ),
        (
            json!({ "op": "not_in", "left": { "value": "t9" }, "right": { "attr": "subject.orgUnitIds" } }),
            true,
        ),
        (
            json!({ "op": "contains", "left": { "attr": "resource.attributes.tags" }, "right": { "value": "a" } }),
            true,
        ),
        (
            json!({ "op": "gt", "left": { "attr": "resource.sensitivityRank" }, "right": { "attr": "subject.clearanceRank" } }),
            true,
        ),
        (
            json!({ "op": "lte", "left": { "attr": "resource.sensitivityRank" }, "right": { "value": 1 } }),
            false,
        ),
        (
            json!({ "op": "exists", "left": { "attr": "context.actorType" } }),
            true,
        ),
    ];
    for (condition, expected) in cases {
        assert_eq!(
            evaluate_condition(&cond(condition.clone()), &env()),
            expected,
            "{condition}"
        );
    }
}

#[test]
fn combines_with_all_any_not() {
    let condition = cond(json!({
        "all": [
            { "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } },
            { "not": { "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": "secret" } } },
            { "any": [
                { "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": "nobody" } },
                { "op": "exists", "left": { "attr": "resource.attributes.status" } }
            ] }
        ]
    }));
    assert!(evaluate_condition(&condition, &env()));
}

#[test]
fn never_matches_unknown_attributes_nor_walks_into_scalars() {
    let missing = cond(json!({ "op": "eq", "left": { "attr": "subject.missing" } }));
    assert!(!evaluate_condition(&missing, &env()));
    let into_string = cond(json!({ "op": "exists", "left": { "attr": "resource.type.length" } }));
    assert!(!evaluate_condition(&into_string, &env()));
    let unknown_neq = cond(
        json!({ "op": "neq", "left": { "attr": "subject.missing" }, "right": { "value": 1 } }),
    );
    assert!(!evaluate_condition(&unknown_neq, &env()));
}

#[test]
fn does_not_compare_numbers_with_non_numbers() {
    let condition = cond(json!({ "op": "gt", "left": { "value": "3" }, "right": { "value": 1 } }));
    assert!(!evaluate_condition(&condition, &env()));
}

#[test]
fn validates_the_json_representation() {
    let parse = |value: Value| serde_json::from_value::<Condition>(value).is_ok();
    assert!(parse(
        json!({ "op": "eq", "left": { "attr": "a" }, "right": { "value": 1 } })
    ));
    assert!(!parse(json!({ "op": "regex", "left": { "attr": "a" } })));
    assert!(!parse(
        json!({ "all": [{ "op": "eq", "left": { "code": "x" } }] })
    ));
    assert!(!parse(json!({ "op": "eq", "left": { "attr": "" } })));
    assert!(!parse(
        json!({ "op": "eq", "left": { "attr": "a" }, "extra": true })
    ));
}
