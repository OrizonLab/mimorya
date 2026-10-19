//! ABAC condition language: operators, combinators and JSON validation.

use mimorya_core::authz::{Condition, evaluate_condition, reads_resource};
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
fn compares_numbers_by_value() {
    // `2.0` stored through the API means 2, as the web editor shows it.
    for (op, right, expected) in [
        ("eq", json!(2.0), true),
        ("neq", json!(2.0), false),
        ("in", json!([0.0, 2.0]), true),
        ("not_in", json!([2.0]), false),
        ("eq", json!(2.5), false),
        ("eq", json!("2"), false),
    ] {
        let condition = cond(
            json!({ "op": op, "left": { "attr": "resource.sensitivityRank" }, "right": { "value": right } }),
        );
        assert_eq!(
            evaluate_condition(&condition, &env()),
            expected,
            "{op} {right}"
        );
    }
    let contains =
        cond(json!({ "op": "contains", "left": { "value": [1, 2] }, "right": { "value": 2.0 } }));
    assert!(evaluate_condition(&contains, &env()));
    // Lists compare item by item, by value too.
    let env = json!({ "subject": { "attributes": { "levels": [1, 2] } } });
    for (op, expected) in [("eq", true), ("neq", false)] {
        let condition = cond(
            json!({ "op": op, "left": { "attr": "subject.attributes.levels" }, "right": { "value": [1, 2.0] } }),
        );
        assert_eq!(evaluate_condition(&condition, &env), expected, "{op}");
    }
    let longer = cond(
        json!({ "op": "eq", "left": { "attr": "subject.attributes.levels" }, "right": { "value": [1, 2, 3] } }),
    );
    assert!(!evaluate_condition(&longer, &env));
}

#[test]
fn a_missing_right_attribute_differs_from_any_value() {
    // A deny "not the owner" holds on a resource without owner.
    let condition = cond(
        json!({ "op": "neq", "left": { "attr": "subject.userId" }, "right": { "attr": "resource.ownerUserId" } }),
    );
    assert!(evaluate_condition(&condition, &env()));
    let equal = cond(
        json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "attr": "resource.ownerUserId" } }),
    );
    assert!(!evaluate_condition(&equal, &env()));
}

#[test]
fn tells_which_conditions_read_the_resource() {
    let person = json!({ "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "administratif" } });
    let reading = [
        json!({ "op": "neq", "left": { "attr": "subject.userId" }, "right": { "attr": "resource.ownerUserId" } }),
        json!({ "all": [person, { "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "planned" } }] }),
        json!({ "any": [person, { "op": "exists", "left": { "attr": "resource.type" } }] }),
        json!({ "not": { "op": "gte", "left": { "attr": "resource.sensitivityRank" }, "right": { "value": 2 } } }),
        json!({ "op": "exists", "left": { "attr": "resource.orgUnitId" } }),
    ];
    for condition in reading {
        assert!(reads_resource(&cond(condition.clone())), "{condition}");
    }
    let not_reading = [
        person.clone(),
        json!({ "all": [person, { "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } }] }),
        json!({ "op": "eq", "left": { "attr": "subject.attributes.resource" }, "right": { "value": "x" } }),
    ];
    for condition in not_reading {
        assert!(!reads_resource(&cond(condition.clone())), "{condition}");
    }
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
