//! The parameters of an OpenAPI operation as a REST operation declares them
//! (ADR-114): its path and query parameters (the path item's ones, unless
//! the operation redefines them) and the properties of its JSON body, each
//! of a simple type. One that cannot be declared (a header, a cookie, an
//! array, an object…) is left aside with a note when it is optional;
//! required, it leaves the whole operation aside: its tool could never make
//! the request the API expects.

use serde_json::Value;

use super::document::Document;
use super::text::{one_line, quoted};
use crate::rest::checks::{MAX_PARAMETER_DESCRIPTION_CHARS, MAX_PARAMETERS, parameter_name};
use crate::rest::declaration::{HttpMethod, ParameterLocation, RestParameter};

/// Parameters read for one operation before any is declared.
const MAX_READ: usize = 200;

/// What an operation may declare, and what was left aside of it.
#[derive(Default)]
pub(super) struct Gathered {
    pub parameters: Vec<RestParameter>,
    pub notes: Vec<String>,
    /// Whether a hidden character was taken out of a description.
    pub stripped: bool,
}

/// One parameter as the document gives it.
struct Given<'a> {
    name: &'a str,
    location: Result<ParameterLocation, &'static str>,
    required: bool,
    /// The schema of its value, or what replaces it.
    schema: Result<&'a Value, &'static str>,
    description: Option<&'a str>,
}

pub(super) fn gather<'a>(
    doc: &mut Document<'a>,
    item: &'a Value,
    operation: &'a Value,
    method: HttpMethod,
) -> Result<Gathered, String> {
    let mut gathered = Gathered::default();
    for given in listed(doc, item, operation)? {
        gathered.add(doc, given)?;
    }
    if let Some(body) = operation.get("requestBody") {
        body_parameters(doc, body, method, &mut gathered)?;
    }
    gathered.fit()?;
    Ok(gathered)
}

/// The path item's parameters, then the operation's, which redefine those
/// of the same name and place where they stand.
fn listed<'a>(
    doc: &mut Document<'a>,
    item: &'a Value,
    operation: &'a Value,
) -> Result<Vec<Given<'a>>, String> {
    let mut listed: Vec<(&str, Given)> = Vec::new();
    for raw in [item, operation]
        .into_iter()
        .filter_map(|part| part.get("parameters").and_then(Value::as_array))
        .flatten()
    {
        let parameter = doc
            .resolve(raw)
            .map_err(|why| format!("un paramètre : {why}"))?;
        let (Some(name), Some(place)) = (parameter["name"].as_str(), parameter["in"].as_str())
        else {
            return Err("un paramètre sans nom ou sans emplacement".into());
        };
        if listed.len() >= MAX_READ {
            return Err(format!("plus de {MAX_READ} paramètres"));
        }
        let location = match place {
            "path" => Ok(ParameterLocation::Path),
            "query" => Ok(ParameterLocation::Query),
            "header" => Err("un en-tête"),
            "cookie" => Err("un cookie"),
            _ => Err("un emplacement inconnu"),
        };
        let schema = match parameter.get("schema") {
            Some(schema) => Ok(schema),
            None => Err("un contenu encodé"),
        };
        let given = Given {
            name,
            required: place == "path" || parameter["required"] == true,
            location,
            schema,
            description: parameter["description"].as_str(),
        };
        // Redefined in its place.
        match listed
            .iter()
            .position(|(at, listed)| *at == place && listed.name == name)
        {
            Some(at) => listed[at] = (place, given),
            None => listed.push((place, given)),
        }
    }
    Ok(listed.into_iter().map(|(_, given)| given).collect())
}

/// The properties of the operation's JSON body.
fn body_parameters<'a>(
    doc: &mut Document<'a>,
    body: &'a Value,
    method: HttpMethod,
    gathered: &mut Gathered,
) -> Result<(), String> {
    let body = doc
        .resolve(body)
        .map_err(|why| format!("son corps : {why}"))?;
    let required = body["required"] == true;
    let aside = |gathered: &mut Gathered, why: String| {
        if required {
            Err(format!("son corps requis : {why}, non pris en charge"))
        } else {
            gathered
                .notes
                .push(format!("corps facultatif laissé de côté : {why}"));
            Ok(())
        }
    };
    if !method.has_body() {
        return aside(
            gathered,
            format!("une requête {} n’en a pas", method.as_str()),
        );
    }
    let content = body.get("content").and_then(Value::as_object);
    let json = content.into_iter().flatten().find(|(media, _)| {
        let media = media.split(';').next().unwrap_or_default().trim();
        media.eq_ignore_ascii_case("application/json") || media.ends_with("+json")
    });
    let Some((_, media)) = json else {
        return aside(gathered, "un contenu autre que JSON".into());
    };
    let Some(schema) = media.get("schema") else {
        return aside(gathered, "un JSON sans schéma".into());
    };
    let object = match doc.object(schema) {
        Ok(object) => object,
        Err(why) => return aside(gathered, why.into()),
    };
    for (name, property) in object.properties {
        let property = doc
            .resolve(property)
            .map_err(|why| format!("le champ « {} » : {why}", quoted(name)))?;
        if property["readOnly"] == true {
            continue;
        }
        gathered.add(
            doc,
            Given {
                name,
                location: Ok(ParameterLocation::Body),
                // Required only when the body is.
                required: required && object.required.contains(name),
                schema: Ok(property),
                description: property["description"].as_str(),
            },
        )?;
    }
    Ok(())
}

impl Gathered {
    fn add<'a>(&mut self, doc: &mut Document<'a>, given: Given<'a>) -> Result<(), String> {
        let refused = given.location.and_then(|location| {
            if !parameter_name(given.name) {
                return Err("un nom refusé");
            }
            if self.parameters.iter().any(|p| p.name == given.name) {
                return Err("un nom déjà pris par un autre paramètre");
            }
            let kind = doc.scalar(given.schema?)?;
            Ok((location, kind))
        });
        let name = quoted(given.name);
        let (location, kind) = match refused {
            Ok(declared) => declared,
            Err(why) if given.required => {
                return Err(format!(
                    "paramètre requis « {name} » : {why}, non pris en charge"
                ));
            }
            Err(why) => {
                self.notes
                    .push(format!("paramètre « {name} » laissé de côté : {why}"));
                return Ok(());
            }
        };
        let schema = given
            .schema
            .ok()
            .and_then(|schema| doc.resolve(schema).ok());
        let mut description = given.description.unwrap_or_default().to_string();
        if let Some(values) = schema.and_then(|s| s.get("enum")).and_then(Value::as_array) {
            let values: Vec<String> = values
                .iter()
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                .collect();
            description = format!("{description} Valeurs : {}.", values.join(", "));
        }
        self.parameters.push(RestParameter {
            name: given.name.to_string(),
            location,
            kind,
            required: given.required,
            description: one_line(
                &description,
                MAX_PARAMETER_DESCRIPTION_CHARS,
                &mut self.stripped,
            ),
        });
        Ok(())
    }

    /// Within the parameters an operation may declare: the required ones,
    /// then the optional ones in the order written.
    fn fit(&mut self) -> Result<(), String> {
        if self.parameters.len() <= MAX_PARAMETERS {
            return Ok(());
        }
        let required = self.parameters.iter().filter(|p| p.required).count();
        if required > MAX_PARAMETERS {
            return Err(format!(
                "{required} paramètres requis, {MAX_PARAMETERS} au plus"
            ));
        }
        let before = self.parameters.len();
        let mut room = MAX_PARAMETERS - required;
        self.parameters.retain(|p| {
            let kept = p.required || room > 0;
            if !p.required && kept {
                room -= 1;
            }
            kept
        });
        self.notes.push(format!(
            "{} paramètres facultatifs laissés de côté ({MAX_PARAMETERS} au plus)",
            before - self.parameters.len()
        ));
        Ok(())
    }
}
