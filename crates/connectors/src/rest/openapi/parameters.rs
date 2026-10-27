//! The parameters of an OpenAPI operation as a REST operation declares them
//! (ADR-114): its path and query parameters (the path item's ones, unless
//! the operation redefines them) and the fields of its JSON body, each of a
//! simple type. One that cannot be declared (a header, a cookie, an array,
//! an object…) is left aside with a note when it is optional; required, it
//! leaves the whole operation aside: its tool could never make the request
//! the API expects. So does a required body of which no field can be
//! declared.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::document::Document;
use super::text::{beginning, one_line, quoted};
use crate::rest::checks::{MAX_PARAMETER_DESCRIPTION_CHARS, MAX_PARAMETERS, parameter_name};
use crate::rest::declaration::{HttpMethod, ParameterLocation, RestParameter};

/// Parameters listed for one operation before any is declared.
const MAX_READ: usize = 200;
/// Values of an enumeration its parameter's description shows.
const MAX_VALUES_SHOWN: usize = 50;

/// What an operation may declare, and what was left aside of it.
#[derive(Default)]
pub(super) struct Gathered {
    pub parameters: Vec<RestParameter>,
    pub notes: Vec<String>,
    /// Whether a hidden character was taken out of a description.
    pub stripped: bool,
    names: HashSet<String>,
    /// Whether the operation requires its body: one of its fields is kept.
    body_required: bool,
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
    let mut listed: Vec<Given> = Vec::new();
    let mut at: HashMap<(&str, &str), usize> = HashMap::new();
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
        doc.spend(name.len())?;
        let (place, location) = match place {
            "path" => ("path", Ok(ParameterLocation::Path)),
            "query" => ("query", Ok(ParameterLocation::Query)),
            "header" => ("header", Err("un en-tête")),
            "cookie" => ("cookie", Err("un cookie")),
            _ => ("other", Err("un emplacement inconnu")),
        };
        let given = Given {
            name,
            required: place == "path" || parameter["required"] == true,
            location,
            schema: parameter.get("schema").ok_or("un contenu encodé"),
            description: parameter["description"].as_str(),
        };
        match at.get(&(place, name)) {
            Some(&index) => listed[index] = given,
            None if listed.len() >= MAX_READ => {
                return Err(format!("plus de {MAX_READ} paramètres"));
            }
            None => {
                at.insert((place, name), listed.len());
                listed.push(given);
            }
        }
    }
    Ok(listed)
}

/// The fields of the operation's JSON body.
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
    let aside = |gathered: &mut Gathered, why: &str| {
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
        let why = format!("une requête {} n’en a pas", method.as_str());
        return aside(gathered, &why);
    }
    let mut json = None;
    for (media, content) in body
        .get("content")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        doc.spend(media.len())?;
        let media = media.split(';').next().unwrap_or_default().trim();
        if media.eq_ignore_ascii_case("application/json") || media.ends_with("+json") {
            json = Some(content);
            break;
        }
    }
    let Some(media) = json else {
        return aside(gathered, "un contenu autre que JSON");
    };
    let Some(schema) = media.get("schema") else {
        return aside(gathered, "un JSON sans schéma");
    };
    let object = match doc.object(schema) {
        Ok(object) => object,
        Err(why) => return aside(gathered, why),
    };
    let before = gathered.parameters.len();
    for (name, property) in object.properties {
        // Required only when the body is.
        let field_required = required && object.required.contains(name);
        let property = match doc.resolve(property) {
            Ok(property) => property,
            Err(why) if field_required => {
                return Err(format!("le champ requis « {} » : {why}", quoted(name)));
            }
            Err(why) => {
                let name = quoted(name);
                gathered
                    .notes
                    .push(format!("champ « {name} » laissé de côté : {why}"));
                continue;
            }
        };
        if property["readOnly"] == true {
            continue;
        }
        let given = Given {
            name,
            location: Ok(ParameterLocation::Body),
            required: field_required,
            schema: Ok(property),
            description: property["description"].as_str(),
        };
        gathered.add(doc, given)?;
    }
    if gathered.parameters.len() == before {
        // Its tool would send no body.
        return aside(gathered, "aucun champ déclarable");
    }
    gathered.body_required = required;
    Ok(())
}

impl Gathered {
    fn add<'a>(&mut self, doc: &mut Document<'a>, given: Given<'a>) -> Result<(), String> {
        let declared = given.location.and_then(|location| {
            if !parameter_name(given.name) {
                return Err("un nom refusé");
            }
            if self.names.contains(given.name) {
                return Err("un nom déjà pris par un autre paramètre");
            }
            let schema = doc.resolve(given.schema?)?;
            Ok((location, doc.scalar(schema)?, schema))
        });
        let (location, kind, schema) = match declared {
            Ok(declared) => declared,
            Err(why) if given.required => {
                let name = quoted(given.name);
                return Err(format!(
                    "paramètre requis « {name} » : {why}, non pris en charge"
                ));
            }
            Err(why) => {
                let name = quoted(given.name);
                self.notes
                    .push(format!("paramètre « {name} » laissé de côté : {why}"));
                return Ok(());
            }
        };
        let description = self.description(doc, given.description, schema)?;
        self.names.insert(given.name.to_string());
        self.parameters.push(RestParameter {
            name: given.name.to_string(),
            location,
            kind,
            required: given.required,
            description,
        });
        Ok(())
    }

    /// What the model reads of a parameter: its description, then the
    /// values it takes, on one line within the bound. A value is read only
    /// as far as there is room left to show it.
    fn description<'a>(
        &mut self,
        doc: &mut Document<'a>,
        written: Option<&'a str>,
        schema: &'a Value,
    ) -> Result<String, String> {
        let max = MAX_PARAMETER_DESCRIPTION_CHARS;
        let mut description = match written {
            Some(text) => one_line(doc.read(text, max)?, max, &mut self.stripped),
            None => String::new(),
        };
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            let mut room = max.saturating_sub(description.chars().count() + " Valeurs : ".len());
            let mut shown = Vec::new();
            for value in values.iter().take(MAX_VALUES_SHOWN) {
                if room == 0 {
                    break;
                }
                let text = match value {
                    Value::String(text) => {
                        one_line(doc.read(text, room)?, room, &mut self.stripped)
                    }
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    _ => continue,
                };
                room = room.saturating_sub(text.chars().count() + ", ".len());
                shown.push(text);
            }
            let more = if values.len() > shown.len() {
                ", …"
            } else {
                ""
            };
            description = format!("{description} Valeurs : {}{more}.", shown.join(", "));
        }
        Ok(one_line(beginning(&description, max), max, &mut false))
    }

    /// Within the parameters an operation may declare: the required ones
    /// (and the first field of a required body none of whose fields is, so
    /// that its tool still sends a body), then the optional ones in the
    /// order written.
    fn fit(&mut self) -> Result<(), String> {
        if self.parameters.len() <= MAX_PARAMETERS {
            return Ok(());
        }
        let in_body = |p: &RestParameter| p.location == ParameterLocation::Body;
        let first_field = (self.body_required
            && !self.parameters.iter().any(|p| in_body(p) && p.required))
        .then(|| self.parameters.iter().position(in_body))
        .flatten();
        let kept: Vec<bool> = (0..self.parameters.len())
            .map(|at| self.parameters[at].required || Some(at) == first_field)
            .collect();
        let required = kept.iter().filter(|kept| **kept).count();
        if required > MAX_PARAMETERS {
            return Err(format!(
                "{required} paramètres requis, {MAX_PARAMETERS} au plus"
            ));
        }
        let before = self.parameters.len();
        let mut room = MAX_PARAMETERS - required;
        let mut at = 0;
        self.parameters.retain(|_| {
            let must = kept[at];
            at += 1;
            let keep = must || room > 0;
            if !must && keep {
                room -= 1;
            }
            keep
        });
        self.notes.push(format!(
            "{} paramètres facultatifs laissés de côté ({MAX_PARAMETERS} au plus)",
            before - self.parameters.len()
        ));
        Ok(())
    }
}
