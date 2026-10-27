//! What the parts of an OpenAPI document stand for: a local `$ref`
//! (`#/components/…`) followed a bounded number of times, never a file nor
//! an address; the type of a schema as a parameter may have one; the
//! properties of an object schema, `allOf` parts merged. Every part read
//! counts against the import's budget, so that references nested in each
//! other cannot make a small document long to read.

use std::collections::HashSet;

use serde_json::Value;

use crate::rest::declaration::ParameterType;

/// References followed for one part: a cycle, or a long chain, ends there.
const MAX_HOPS: usize = 10;
/// `allOf` parts within parts: the reading recurses as deep.
const MAX_ALL_OF_DEPTH: usize = 4;
/// Why a part cannot be read once the budget is spent.
pub(super) const TOO_COMPLEX: &str = "un document trop complexe";

/// The document and how many more parts may be read of it.
pub(super) struct Document<'a> {
    root: &'a Value,
    budget: usize,
}

/// An object schema's properties, in the order written, and the names it
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

    fn spend(&mut self) -> Result<(), &'static str> {
        self.budget = self.budget.checked_sub(1).ok_or(TOO_COMPLEX)?;
        Ok(())
    }

    /// The part a value stands for: itself, or what its `$ref` points to in
    /// this document.
    pub(super) fn resolve(&mut self, mut value: &'a Value) -> Result<&'a Value, &'static str> {
        for _ in 0..MAX_HOPS {
            self.spend()?;
            let Some(reference) = value.get("$ref") else {
                return Ok(value);
            };
            value = reference
                .as_str()
                .and_then(|reference| reference.strip_prefix('#'))
                .and_then(|pointer| self.root.pointer(pointer))
                .ok_or("une référence hors du document ou introuvable")?;
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
            Value::Array(kinds) => {
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

    /// The properties of an object schema and those of its `allOf` parts,
    /// or what the schema is instead.
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
            self.spend()?;
            // The first part to name a property describes it.
            if into.names.insert(name) {
                into.properties.push((name, property));
            }
        }
        let required = schema.get("required").and_then(Value::as_array);
        into.required
            .extend(required.into_iter().flatten().filter_map(Value::as_str));
        Ok(())
    }
}
