//! Typed tools: the input is a Rust struct whose serde, schemars and validator
//! attributes give, from one definition, the JSON Schema shown to the model
//! and the strict validation applied before anything runs.

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

use super::contract::{Tool, ToolContext, ToolInfo, ToolRef, ToolResult};

#[async_trait]
pub trait TypedTool: Send + Sync + 'static {
    /// Use `#[serde(deny_unknown_fields)]`: unexpected fields are rejected.
    type Input: DeserializeOwned + Serialize + JsonSchema + Validate + Send + Sync;

    fn info(&self) -> ToolInfo;

    fn describe_action(&self, _input: &Self::Input) -> Option<String> {
        None
    }

    async fn run(&self, input: Self::Input, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}

/// Wraps a typed tool into the erased contract used by registries.
pub fn typed_tool<T: TypedTool>(tool: T) -> ToolRef {
    Arc::new(Typed {
        info: tool.info(),
        schema: input_schema::<T::Input>(),
        tool,
        _input: PhantomData,
    })
}

/// JSON Schema of an input type, inlined (no `$ref`) and without meta keys.
pub fn input_schema<I: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let mut schema = generator.into_root_schema_for::<I>().to_value();
    if let Value::Object(map) = &mut schema {
        map.remove("$schema");
        map.remove("title");
    }
    schema
}

struct Typed<T: TypedTool> {
    info: ToolInfo,
    schema: Value,
    tool: T,
    _input: PhantomData<fn() -> T::Input>,
}

#[async_trait]
impl<T: TypedTool> Tool for Typed<T> {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    fn parse_input(&self, raw: &Value) -> Result<Value, Vec<String>> {
        let input: T::Input = serde_json::from_value(raw.clone())
            .map_err(|error| vec![format!("(racine) : structure invalide — {error}")])?;
        input
            .validate()
            .map_err(|errors| validation_issues(&errors))?;
        serde_json::to_value(&input).map_err(|error| vec![error.to_string()])
    }

    fn describe_action(&self, input: &Value) -> Option<String> {
        let input = serde_json::from_value::<T::Input>(input.clone()).ok()?;
        self.tool.describe_action(&input)
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let input = serde_json::from_value::<T::Input>(input)?;
        self.tool.run(input, ctx).await
    }
}

/// Validation errors as French messages prefixed by the field path.
pub fn validation_issues(errors: &ValidationErrors) -> Vec<String> {
    let mut issues = Vec::new();
    collect_issues(errors, "", &mut issues);
    issues.sort();
    issues
}

fn collect_issues(errors: &ValidationErrors, prefix: &str, issues: &mut Vec<String>) {
    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            camel_case(field)
        } else {
            format!("{prefix}.{}", camel_case(field))
        };
        match kind {
            ValidationErrorsKind::Field(list) => {
                for error in list {
                    issues.push(format!("{path} : {}", describe(error)));
                }
            }
            ValidationErrorsKind::Struct(nested) => collect_issues(nested, &path, issues),
            ValidationErrorsKind::List(items) => {
                for (index, nested) in items {
                    collect_issues(nested, &format!("{path}.{index}"), issues);
                }
            }
        }
    }
}

fn describe(error: &validator::ValidationError) -> String {
    if let Some(message) = &error.message {
        return message.to_string();
    }
    let param = |name: &str| error.params.get(name).map(ToString::to_string);
    match error.code.as_ref() {
        "length" => match (param("min"), param("max")) {
            (Some(min), Some(max)) => format!("longueur attendue entre {min} et {max}"),
            (Some(min), None) => format!("longueur minimale {min}"),
            (None, Some(max)) => format!("longueur maximale {max}"),
            _ => "longueur invalide".into(),
        },
        "range" => match (param("min"), param("max")) {
            (Some(min), Some(max)) => format!("valeur attendue entre {min} et {max}"),
            (Some(min), None) => format!("valeur minimale {min}"),
            (None, Some(max)) => format!("valeur maximale {max}"),
            _ => "valeur hors limites".into(),
        },
        "regex" => "format invalide".into(),
        "email" => "adresse électronique invalide".into(),
        code => format!("valeur invalide ({code})"),
    }
}

fn camel_case(field: &str) -> String {
    let mut result = String::with_capacity(field.len());
    let mut upper = false;
    for c in field.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            result.extend(c.to_uppercase());
            upper = false;
        } else {
            result.push(c);
        }
    }
    result
}
