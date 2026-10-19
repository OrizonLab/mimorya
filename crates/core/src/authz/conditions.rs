//! Minimal ABAC condition language, serialisable as JSON so that tenants can
//! store policies in the database. Deliberately small: no functions, no regex,
//! no arbitrary code — conditions must be auditable by an administrator.
//!
//! Attributes are addressed by dotted paths in an evaluation environment:
//!   subject.*  (userId, orgUnitIds, clearanceRank, attributes.*)
//!   resource.* (type, ownerUserId, orgUnitId, sensitivityRank, attributes.*)
//!   context.*  (actorType, channel, …)

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    All(AllOf),
    Any(AnyOf),
    Not(NotOf),
    Compare(Comparison),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllOf {
    pub all: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnyOf {
    pub any: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotOf {
    pub not: Box<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub op: ComparisonOp,
    pub left: Operand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Operand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    Eq,
    Neq,
    In,
    NotIn,
    Contains,
    Gt,
    Gte,
    Lt,
    Lte,
    Exists,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Operand {
    Attr(AttrRef),
    Value(Literal),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttrRef {
    #[serde(deserialize_with = "non_empty")]
    pub attr: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Literal {
    pub value: Value,
}

fn non_empty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("attribute path must not be empty"));
    }
    Ok(value)
}

impl Condition {
    pub fn all(conditions: Vec<Condition>) -> Self {
        Self::All(AllOf { all: conditions })
    }

    pub fn any(conditions: Vec<Condition>) -> Self {
        Self::Any(AnyOf { any: conditions })
    }

    pub fn negate(condition: Condition) -> Self {
        Self::Not(NotOf {
            not: Box::new(condition),
        })
    }

    pub fn compare(op: ComparisonOp, left: Operand, right: Option<Operand>) -> Self {
        Self::Compare(Comparison { op, left, right })
    }
}

impl Operand {
    pub fn attr(path: impl Into<String>) -> Self {
        Self::Attr(AttrRef { attr: path.into() })
    }

    pub fn value(value: impl Into<Value>) -> Self {
        Self::Value(Literal {
            value: value.into(),
        })
    }
}

/// Whether the condition reads an attribute of the resource: such a condition
/// can only be evaluated where a resource is checked.
pub fn reads_resource(condition: &Condition) -> bool {
    let is_resource = |operand: &Operand| matches!(operand, Operand::Attr(a) if a.attr == "resource" || a.attr.starts_with("resource."));
    match condition {
        Condition::All(c) => c.all.iter().any(reads_resource),
        Condition::Any(c) => c.any.iter().any(reads_resource),
        Condition::Not(c) => reads_resource(&c.not),
        Condition::Compare(c) => is_resource(&c.left) || c.right.as_ref().is_some_and(is_resource),
    }
}

/// Walks object keys (and array indexes) only; anything else resolves to nothing.
fn resolve_path<'a>(env: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(env, |current, key| match current {
        Value::Object(map) => map.get(key),
        Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
        _ => None,
    })
}

fn resolve<'a>(env: &'a Value, operand: Option<&'a Operand>) -> Option<&'a Value> {
    match operand? {
        Operand::Attr(attr) => resolve_path(env, &attr.attr),
        Operand::Value(literal) => Some(&literal.value),
    }
}

/// Equality by value: `2` and `2.0` are the same number, as an administrator
/// (and the web editor, which reads both as `2`) expects.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            x == y || ((x.is_f64() || y.is_f64()) && x.as_f64() == y.as_f64())
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        _ => a == b,
    }
}

fn holds(items: &[Value], item: &Value) -> bool {
    items.iter().any(|candidate| same(candidate, item))
}

/// Numbers only: `"3" > 1` is false.
fn compare_numbers(left: Option<&Value>, right: Option<&Value>, f: fn(f64, f64) -> bool) -> bool {
    match (left.and_then(Value::as_f64), right.and_then(Value::as_f64)) {
        (Some(a), Some(b)) => f(a, b),
        _ => false,
    }
}

/// Evaluates a condition. Unknown attributes resolve to nothing and never match.
pub fn evaluate_condition(condition: &Condition, env: &Value) -> bool {
    let comparison = match condition {
        Condition::All(c) => return c.all.iter().all(|c| evaluate_condition(c, env)),
        Condition::Any(c) => return c.any.iter().any(|c| evaluate_condition(c, env)),
        Condition::Not(c) => return !evaluate_condition(&c.not, env),
        Condition::Compare(comparison) => comparison,
    };

    let left = resolve(env, Some(&comparison.left));
    let right = resolve(env, comparison.right.as_ref());

    match comparison.op {
        ComparisonOp::Exists => left.is_some_and(|v| !v.is_null()),
        ComparisonOp::Eq => matches!((left, right), (Some(a), Some(b)) if same(a, b)),
        ComparisonOp::Neq => left.is_some_and(|a| !right.is_some_and(|b| same(a, b))),
        ComparisonOp::In => match (left, right.and_then(Value::as_array)) {
            (Some(a), Some(items)) => holds(items, a),
            _ => false,
        },
        ComparisonOp::NotIn => match (left, right.and_then(Value::as_array)) {
            (Some(a), Some(items)) => !holds(items, a),
            _ => false,
        },
        ComparisonOp::Contains => match (left.and_then(Value::as_array), right) {
            (Some(items), Some(b)) => holds(items, b),
            _ => false,
        },
        ComparisonOp::Gt => compare_numbers(left, right, |a, b| a > b),
        ComparisonOp::Gte => compare_numbers(left, right, |a, b| a >= b),
        ComparisonOp::Lt => compare_numbers(left, right, |a, b| a < b),
        ComparisonOp::Lte => compare_numbers(left, right, |a, b| a <= b),
    }
}
