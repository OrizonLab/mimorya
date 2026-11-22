//! The production stack as `docker compose` reads it, as far as the tests
//! need: the variables `compose.production.yml` gives the services that run
//! the configuration, and their `${…}` filled from an env file.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::{Captures, Regex};

/// The services that read the configuration.
pub const BACKEND: [&str; 2] = ["api", "worker"];

pub fn repository(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} unreadable", path.display()))
}

/// Where a line of the compose file stands.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Elsewhere,
    /// The block the backend services merge (`<<: *backend-environment`).
    Shared,
    Service,
    Environment,
}

/// What `service` is given, as written: the shared block, then its own
/// `environment`, which overrides it.
pub fn environment(compose: &str, service: &str) -> BTreeMap<String, String> {
    let entry = Regex::new(r"^\s+([A-Z][A-Z0-9_]*):\s*(.*?)\s*$").unwrap();
    let header = format!("  {service}:");
    let mut given = BTreeMap::new();
    let mut place = Place::Elsewhere;
    for line in compose.lines() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let depth = line.len() - line.trim_start().len();
        place = if depth == 0 {
            if line.starts_with("x-backend-environment:") {
                Place::Shared
            } else {
                Place::Elsewhere
            }
        } else if place == Place::Shared {
            Place::Shared
        } else if depth == 2 {
            if line.trim_end() == header {
                Place::Service
            } else {
                Place::Elsewhere
            }
        } else if depth == 4 && matches!(place, Place::Service | Place::Environment) {
            if text == "environment:" {
                Place::Environment
            } else {
                Place::Service
            }
        } else {
            place
        };
        if matches!(place, Place::Shared | Place::Environment)
            && let Some(found) = entry.captures(line)
        {
            let value = found[2].trim_matches(|c| c == '\'' || c == '"');
            given.insert(found[1].to_string(), value.to_string());
        }
    }
    given
}

/// The variables an env file sets, not those it describes in a comment.
pub fn set_in(env_file: &str) -> BTreeMap<String, String> {
    let line = Regex::new(r"^([A-Z][A-Z0-9_]*)=(.*)$").unwrap();
    env_file
        .lines()
        .filter_map(|text| line.captures(text))
        .map(|found| (found[1].to_string(), found[2].trim().to_string()))
        .collect()
}

/// `value` once its `${…}` are filled from `env`, as compose fills them: an
/// empty variable counts as unset for `:-` and `:?`; the message of the
/// first `:?` left unset otherwise.
pub fn filled(value: &str, env: &BTreeMap<String, String>) -> Result<String, String> {
    let reference = Regex::new(r"\$\{([A-Z][A-Z0-9_]*)(?::([-?])([^}]*))?\}").unwrap();
    let mut refused = None;
    let value = reference.replace_all(value, |found: &Captures| {
        let set = env.get(&found[1]).filter(|value| !value.is_empty());
        match (set, found.get(2).map(|sign| sign.as_str())) {
            (Some(value), _) => value.clone(),
            (None, Some("-")) => found[3].to_string(),
            (None, Some(_)) => {
                refused.get_or_insert_with(|| found[3].to_string());
                String::new()
            }
            (None, None) => String::new(),
        }
    });
    match refused {
        Some(message) => Err(message),
        None => Ok(value.into_owned()),
    }
}

/// What `service` receives once the stack is started with `env`.
pub fn received(
    compose: &str,
    service: &str,
    env: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    environment(compose, service)
        .into_iter()
        .map(|(name, value)| Ok((name, filled(&value, env)?)))
        .collect()
}

#[test]
fn the_stack_is_read_as_compose_reads_it() {
    let compose = "\
x-backend-environment: &backend-environment
  SHARED: ${SHARED:-un}
  # A comment.
  REQUIRED: ${REQUIRED:?say it}

services:
  db:
    environment:
      DB_ONLY: x
  api:
    image: api
    environment:
      <<: *backend-environment
      ROLE: 'api'
    depends_on:
      db:
        condition: service_healthy
";
    let api = environment(compose, "api");
    assert_eq!(
        api.keys().collect::<Vec<_>>(),
        ["REQUIRED", "ROLE", "SHARED"]
    );
    assert_eq!(api["ROLE"], "api");
    assert!(!environment(compose, "worker").contains_key("ROLE"));
    let mut env = BTreeMap::from([("REQUIRED".to_string(), String::new())]);
    assert_eq!(received(compose, "api", &env), Err("say it".to_string()));
    env.insert("REQUIRED".into(), "deux".into());
    let given = received(compose, "api", &env).unwrap();
    assert_eq!(
        (given["SHARED"].as_str(), given["REQUIRED"].as_str()),
        ("un", "deux")
    );
    assert_eq!(
        set_in("A=1\n# B=2\nC=\n"),
        BTreeMap::from([
            ("A".to_string(), "1".to_string()),
            ("C".to_string(), String::new()),
        ])
    );
}
