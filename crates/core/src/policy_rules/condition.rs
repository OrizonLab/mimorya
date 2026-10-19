//! Checks a policy condition before it is stored: known attribute roots, at
//! least one attribute per comparison, the operands each operator needs, small
//! literal values, bounded depth and length. What is stored is then always
//! readable by the engine (an unreadable policy would stop the company's
//! authorization from loading).

use serde_json::Value;

use crate::authz::{ComparisonOp, Condition, Operand};
use crate::plain_text::{canonical, is_hidden};

pub const MAX_CONDITION_COMPARISONS: usize = 40;
pub const MAX_CONDITION_DEPTH: usize = 6;
const MAX_ATTRIBUTE_CHARS: usize = 200;
const MAX_TEXT_CHARS: usize = 500;
const MAX_LIST_ITEMS: usize = 100;

/// Where an attribute path may start (see `authz::conditions`).
pub const ATTRIBUTE_ROOTS: &[&str] = &["subject", "resource", "context"];

/// Why a condition is refused (in French); `None` when it can be stored.
pub fn condition_problem(condition: &Condition) -> Option<String> {
    let mut comparisons = 0;
    check(condition, 1, &mut comparisons)
}

fn check(condition: &Condition, depth: usize, comparisons: &mut usize) -> Option<String> {
    if depth > MAX_CONDITION_DEPTH {
        return Some(format!(
            "Condition trop imbriquée ({MAX_CONDITION_DEPTH} niveaux au plus)"
        ));
    }
    let group = match condition {
        Condition::All(c) => &c.all,
        Condition::Any(c) => &c.any,
        Condition::Not(c) => return check(&c.not, depth + 1, comparisons),
        Condition::Compare(comparison) => {
            *comparisons += 1;
            if *comparisons > MAX_CONDITION_COMPARISONS {
                return Some(format!(
                    "Condition trop longue ({MAX_CONDITION_COMPARISONS} comparaisons au plus)"
                ));
            }
            return comparison_problem(comparison.op, &comparison.left, comparison.right.as_ref());
        }
    };
    if group.is_empty() {
        return Some("Un groupe de conditions ne peut pas être vide".into());
    }
    group.iter().find_map(|c| check(c, depth + 1, comparisons))
}

fn comparison_problem(op: ComparisonOp, left: &Operand, right: Option<&Operand>) -> Option<String> {
    for operand in [Some(left), right].into_iter().flatten() {
        let problem = match operand {
            Operand::Attr(attr) => attribute_problem(&attr.attr),
            Operand::Value(literal) => value_problem(&literal.value),
        };
        if problem.is_some() {
            return problem;
        }
    }
    let is_attr = |o: &Operand| matches!(o, Operand::Attr(_));
    if !is_attr(left) && !right.is_some_and(is_attr) {
        return Some("Une comparaison porte sur au moins un attribut".into());
    }
    match op {
        ComparisonOp::Exists if right.is_some() => {
            Some("« est renseigné » ne prend pas de valeur à comparer".into())
        }
        ComparisonOp::Exists => None,
        _ if right.is_none() => Some("Valeur à comparer manquante".into()),
        ComparisonOp::In | ComparisonOp::NotIn if literal(right).is_some_and(|v| !v.is_array()) => {
            Some("« fait partie de » attend une liste de valeurs".into())
        }
        ComparisonOp::Gt | ComparisonOp::Gte | ComparisonOp::Lt | ComparisonOp::Lte
            if [literal(Some(left)), literal(right)]
                .into_iter()
                .flatten()
                .any(|v| !v.is_number()) =>
        {
            Some("Les comparaisons d’ordre portent sur des nombres".into())
        }
        _ => None,
    }
}

/// The value of a literal operand.
fn literal(operand: Option<&Operand>) -> Option<&Value> {
    match operand {
        Some(Operand::Value(v)) => Some(&v.value),
        _ => None,
    }
}

/// A literal is a short scalar, or a short list of them.
fn value_problem(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) if items.len() > MAX_LIST_ITEMS => Some(format!(
            "Liste trop longue ({MAX_LIST_ITEMS} valeurs au plus)"
        )),
        Value::Array(items) => items.iter().find_map(|item| match item {
            Value::Array(_) | Value::Object(_) => {
                Some("Une liste ne contient que des valeurs simples".into())
            }
            scalar => value_problem(scalar),
        }),
        Value::Object(_) => {
            Some("Une valeur à comparer est un texte, un nombre ou une liste".into())
        }
        Value::String(text) => text_problem(text),
        _ => None,
    }
}

fn text_problem(text: &str) -> Option<String> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Some(format!(
            "Valeur trop longue ({MAX_TEXT_CHARS} caractères au plus)"
        ));
    }
    // Invisible, it would make a value differ from what is shown (and
    // PostgreSQL cannot store the null character).
    text.chars()
        .any(is_hidden)
        .then(|| "Caractère invisible ou de contrôle interdit dans une valeur".into())
}

/// The condition with its text values in canonical form (NFC), the form
/// collaborator attributes are stored in: a value pasted decomposed still
/// matches.
pub fn canonical_condition(condition: Condition) -> Condition {
    match condition {
        Condition::All(mut c) => {
            c.all = c.all.into_iter().map(canonical_condition).collect();
            Condition::All(c)
        }
        Condition::Any(mut c) => {
            c.any = c.any.into_iter().map(canonical_condition).collect();
            Condition::Any(c)
        }
        Condition::Not(mut c) => {
            c.not = Box::new(canonical_condition(*c.not));
            Condition::Not(c)
        }
        Condition::Compare(mut c) => {
            c.left = canonical_operand(c.left);
            c.right = c.right.map(canonical_operand);
            Condition::Compare(c)
        }
    }
}

fn canonical_operand(operand: Operand) -> Operand {
    match operand {
        Operand::Value(mut literal) => {
            literal.value = canonical_value(literal.value);
            Operand::Value(literal)
        }
        attribute => attribute,
    }
}

fn canonical_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(canonical(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(canonical_value).collect()),
        other => other,
    }
}

fn attribute_problem(path: &str) -> Option<String> {
    let mut segments = path.split('.');
    let root_known = segments
        .next()
        .is_some_and(|root| ATTRIBUTE_ROOTS.contains(&root));
    let well_formed =
        path.len() <= MAX_ATTRIBUTE_CHARS && path.split('.').count() >= 2 && !path.contains('\0');
    if !root_known || !well_formed || segments.any(str::is_empty) {
        return Some(format!(
            "Attribut invalide : « {path} » (il commence par subject., resource. ou context.)"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn condition(value: serde_json::Value) -> Condition {
        serde_json::from_value(value).expect("well-formed condition")
    }

    #[test]
    fn checks_conditions_before_they_are_stored() {
        let department = json!({ "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "administratif" } });
        assert_eq!(condition_problem(&condition(department.clone())), None);
        let listed = json!({ "all": [department, { "op": "in", "left": { "attr": "resource.attributes.status" }, "right": { "value": ["planned", 3] } }] });
        assert_eq!(condition_problem(&condition(listed)), None);
        let long_text = "x".repeat(MAX_TEXT_CHARS + 1);
        let refused = [
            json!({ "all": [] }),
            json!({ "op": "eq", "left": { "attr": "user.department" }, "right": { "value": "x" } }),
            json!({ "op": "eq", "left": { "attr": "subject" }, "right": { "value": "x" } }),
            json!({ "op": "eq", "left": { "attr": "subject..x" }, "right": { "value": "x" } }),
            json!({ "op": "exists", "left": { "attr": "resource.attributes.st\u{0}tus" } }),
            json!({ "op": "eq", "left": { "value": 1 }, "right": { "value": 1 } }),
            json!({ "op": "eq", "left": { "attr": "resource.type" } }),
            json!({ "op": "exists", "left": { "attr": "resource.type" }, "right": { "value": 1 } }),
            json!({ "op": "in", "left": { "attr": "resource.type" }, "right": { "value": "intervention" } }),
            json!({ "op": "gt", "left": { "attr": "resource.sensitivityRank" }, "right": { "value": "2" } }),
            json!({ "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": long_text } }),
            json!({ "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": "a\u{0}b" } }),
            json!({ "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "admini\u{200B}stratif" } }),
            json!({ "op": "in", "left": { "attr": "subject.attributes.site" }, "right": { "value": ["Lille", "Ar\u{034F}ras"] } }),
            json!({ "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": { "attr": "x" } } }),
            json!({ "op": "in", "left": { "attr": "resource.type" }, "right": { "value": [["nested"]] } }),
            json!({ "op": "in", "left": { "attr": "resource.type" }, "right": { "value": vec![1; MAX_LIST_ITEMS + 1] } }),
        ];
        for value in refused {
            assert!(
                condition_problem(&condition(value.clone())).is_some(),
                "{value}"
            );
        }
        let mut deep = json!({ "op": "exists", "left": { "attr": "resource.type" } });
        for _ in 0..MAX_CONDITION_DEPTH {
            deep = json!({ "not": deep });
        }
        assert!(condition_problem(&condition(deep)).is_some());
        let one = json!({ "op": "exists", "left": { "attr": "resource.type" } });
        let long = json!({ "any": vec![one; MAX_CONDITION_COMPARISONS + 1] });
        assert!(condition_problem(&condition(long)).is_some());
    }

    #[test]
    fn stores_text_values_in_canonical_form() {
        let decomposed = condition(json!({ "all": [
            { "op": "eq", "left": { "attr": "subject.attributes.site" }, "right": { "value": "Ze\u{301}lande" } },
            { "not": { "op": "in", "left": { "attr": "resource.attributes.city" }, "right": { "value": ["Ze\u{301}lande", 3] } } }
        ] }));
        let composed = condition(json!({ "all": [
            { "op": "eq", "left": { "attr": "subject.attributes.site" }, "right": { "value": "Z\u{e9}lande" } },
            { "not": { "op": "in", "left": { "attr": "resource.attributes.city" }, "right": { "value": ["Z\u{e9}lande", 3] } } }
        ] }));
        assert_eq!(canonical_condition(decomposed), composed);
    }
}
