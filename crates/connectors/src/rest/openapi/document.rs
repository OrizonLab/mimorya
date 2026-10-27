//! What the parts of an OpenAPI document stand for: a local `$ref`
//! (`#/components/…`) followed a bounded number of times, never a file nor
//! an address; the type of a schema as a parameter may have one; the
//! fields of an object schema, `allOf` parts merged; the beginning of a
//! text. Every part read is paid for out of the import's budget, by its
//! size, each time it is read: a part that many others share through
//! `$ref` costs as often as it is read again, so that a small document
//! cannot make its reading long.

use std::collections::HashSet;

use serde_json::Value;

use super::text::{Read, beginning};
use crate::rest::declaration::ParameterType;

/// References followed for one part: a cycle, or a long chain, ends there.
const MAX_HOPS: usize = 10;
/// `allOf` parts within parts: the reading recurses as deep.
const MAX_ALL_OF_DEPTH: usize = 4;
/// A pointer within a description is short.
const MAX_REFERENCE_BYTES: usize = 512;
/// Fields read of one object schema, its `allOf` parts included.
const MAX_FIELDS: usize = 200;
/// Types an OpenAPI 3.1 schema may list (`["integer", "null"]`).
const MAX_TYPES: usize = 4;
/// What a part costs beyond its first unit: one more per this many bytes.
const BYTES_PER_UNIT: usize = 64;
/// Why a part cannot be read once the budget is spent.
pub(super) const TOO_COMPLEX: &str = "un document trop complexe";

/// The document and how much more may be read of it.
pub(super) struct Document<'a> {
    root: &'a Value,
    budget: usize,
}

/// An object schema's fields, in the order written, and the names it
/// requires.
#[derive(Default)]
pub(super) struct Object<'a> {
    pub properties: Vec<(&'a str, &'a Value)>,
    pub required: HashSet<&'a str>,
    names: HashSet<&'a str>,
}

impl<'a> Document<'a> {
    pub(super) fn new(root: &'a Value, budget: usize) -> Self {
        Self { root, budget }
    }

    pub(super) fn exhausted(&self) -> bool {
        self.budget == 0
    }

    /// Pays for reading `bytes` bytes of the document: one unit, and one
    /// more per 64 bytes. Once the budget is spent, nothing more is read.
    pub(super) fn spend(&mut self, bytes: usize) -> Result<(), &'static str> {
        let cost = 1 + bytes / BYTES_PER_UNIT;
        match self.budget.checked_sub(cost) {
            Some(left) => {
                self.budget = left;
                Ok(())
            }
            None => {
                self.budget = 0;
                Err(TOO_COMPLEX)
            }
        }
    }

    /// The beginning of a text that a bound of `max` characters may need,
    /// paid for.
    pub(super) fn read(&mut self, text: &'a str, max: usize) -> Result<Read<'a>, &'static str> {
        let read = beginning(text, max);
        self.spend(read.bytes())?;
        Ok(read)
    }

    /// The part a value stands for: itself, or what its `$ref` points to in
    /// this document.
    pub(super) fn resolve(&mut self, mut value: &'a Value) -> Result<&'a Value, &'static str> {
        const ELSEWHERE: &str = "une référence hors du document ou introuvable";
        for _ in 0..MAX_HOPS {
            let Some(reference) = value.get("$ref") else {
                self.spend(0)?;
                return Ok(value);
            };
            let reference = reference.as_str().ok_or(ELSEWHERE)?;
            if reference.len() > MAX_REFERENCE_BYTES {
                return Err("une référence trop longue");
            }
            self.spend(reference.len())?;
            value = reference
                .strip_prefix('#')
                .and_then(|pointer| self.root.pointer(pointer))
                .ok_or(ELSEWHERE)?;
        }
        Err("des références en boucle ou trop enchaînées")
    }

    /// The type of a parameter of this schema, or what the schema is
    /// instead (« un tableau »…). A schema without a type, as `{}` or an
    /// enumeration, is text.
    pub(super) fn scalar(&mut self, schema: &'a Value) -> Result<ParameterType, &'static str> {
        let schema = self.resolve(schema)?;
        let kind = match &schema["type"] {
            Value::String(kind) => Some(kind.as_str()),
            // OpenAPI 3.1: `["integer", "null"]`.
            Value::Array(kinds) if kinds.len() <= MAX_TYPES => {
                let named: Vec<&str> = kinds
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|kind| *kind != "null")
                    .collect();
                match named.as_slice() {
                    [] => None,
                    [kind] => Some(*kind),
                    _ => return Err("plusieurs types"),
                }
            }
            Value::Array(_) => return Err("plusieurs types"),
            _ => None,
        };
        match kind {
            Some("string") => Ok(ParameterType::String),
            Some("integer") => Ok(ParameterType::Integer),
            Some("number") => Ok(ParameterType::Number),
            Some("boolean") => Ok(ParameterType::Boolean),
            Some("array") => Err("un tableau"),
            Some("object") => Err("un objet"),
            Some(_) => Err("un type inconnu"),
            None if ["oneOf", "anyOf", "allOf"]
                .iter()
                .any(|k| schema.get(k).is_some()) =>
            {
                Err("des variantes (oneOf, anyOf, allOf)")
            }
            None if schema.get("properties").is_some() => Err("un objet"),
            None => Ok(ParameterType::String),
        }
    }

    /// The fields of an object schema and those of its `allOf` parts, or
    /// what the schema is instead.
    pub(super) fn object(&mut self, schema: &'a Value) -> Result<Object<'a>, &'static str> {
        let mut object = Object::default();
        self.merge(schema, 0, &mut object)?;
        Ok(object)
    }

    fn merge(
        &mut self,
        schema: &'a Value,
        depth: usize,
        into: &mut Object<'a>,
    ) -> Result<(), &'static str> {
        let schema = self.resolve(schema)?;
        if ["oneOf", "anyOf"].iter().any(|k| schema.get(k).is_some()) {
            return Err("des variantes (oneOf, anyOf)");
        }
        if schema.get("type").is_some_and(|kind| kind != "object") {
            return Err("autre chose qu’un objet");
        }
        if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
            if depth >= MAX_ALL_OF_DEPTH {
                return Err("des allOf trop imbriqués");
            }
            for part in parts {
                self.merge(part, depth + 1, into)?;
            }
        }
        for (name, property) in schema
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            self.spend(name.len())?;
            // The first part to name a field describes it.
            if into.names.insert(name) {
                if into.properties.len() >= MAX_FIELDS {
                    return Err("plus de 200 champs");
                }
                into.properties.push((name, property));
            }
        }
        let required = schema.get("required").and_then(Value::as_array);
        for entry in required.into_iter().flatten() {
            let name = entry.as_str();
            self.spend(name.map_or(0, str::len))?;
            into.required.extend(name);
        }
        Ok(())
    }
}
