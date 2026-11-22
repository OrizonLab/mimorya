//! The configuration files never drift from what the API reads (ADR-221):
//! every variable the configuration reads is described in `.env.example`
//! (development), in `.env.production.example` and passed by
//! `compose.production.yml`, but the exceptions listed below, each with its
//! reason; every variable of `.env.production.example` reaches the stack,
//! and every one the stack reads is described there. The variables read are
//! recorded as the configuration asks for them, not listed by hand. The
//! stack itself, started from the example, is in `starts`.

#[path = "configuration_files/stack.rs"]
mod stack;
#[path = "configuration_files/starts.rs"]
mod starts;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;

use mimorya_api::AppConfig;
use regex::Regex;
use stack::{BACKEND, environment, repository};

/// Read by the configuration, fixed by `compose.production.yml` itself: the
/// operator never sets them.
const FIXED_BY_THE_STACK: [(&str, &str); 7] = [
    ("MIMORYA_ENV", "production, for every process of the stack"),
    (
        "SESSION_SECURE_COOKIE",
        "true: the stack is only served over HTTPS, by Caddy",
    ),
    ("MIMORYA_ROLE", "api or worker, by the service"),
    ("DATABASE_APP_URL", "built from MIMORYA_APP_DB_PASSWORD"),
    ("WEB_ORIGIN", "https:// and MIMORYA_DOMAIN"),
    ("TRUST_PROXY", "true for the api, behind Caddy"),
    (
        "MIMORYA_BRAND_DIR",
        "/etc/mimorya/brand, where the stack mounts MIMORYA_BRAND_FOLDER",
    ),
];

/// Read by the configuration, held by the image: neither the operator nor
/// the stack sets them.
const HELD_BY_THE_IMAGE: [(&str, &str); 4] = [
    ("HOST", "0.0.0.0, where Caddy reaches the api"),
    ("PORT", "4000, where Caddy reaches the api"),
    (
        "MIMORYA_EXTRACTOR",
        "mimorya-extract, beside the API in the image",
    ),
    (
        "MIMORYA_MCP_ALLOW_STDIO",
        "a stdio server is a program the published images do not carry",
    ),
];

/// The variables the configuration reads, recorded as it asks for them:
/// with nothing set, then with every branch that reads more opened (a
/// model provider, the operator's proxy, the mail relay).
fn read_by_the_configuration() -> BTreeSet<String> {
    let asked = RefCell::new(BTreeSet::new());
    let opened: &[(&str, &str)] = &[
        ("LLM_PROVIDER", "anthropic"),
        ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
        ("MIMORYA_OUTBOUND_PROXY", "http://proxy.example:3128"),
        ("SMTP_URL", "smtp://localhost:1025"),
        ("MAIL_FROM", "Mimorya <no-reply@example.com>"),
    ];
    for set in [&[][..], opened] {
        let _ = AppConfig::from_vars(|key| {
            asked.borrow_mut().insert(key.to_string());
            set.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
        });
    }
    asked.into_inner()
}

/// The variables an env file sets or describes: `NAME=`, commented or not.
fn described(text: &str) -> BTreeSet<String> {
    let line = Regex::new(r"^#?\s*([A-Z][A-Z0-9_]*)=").unwrap();
    text.lines()
        .filter_map(|text| line.captures(text))
        .map(|found| found[1].to_string())
        .collect()
}

/// The variables the stack gives the services that read the
/// configuration, and those alone.
fn given(compose: &str) -> BTreeSet<String> {
    BACKEND
        .iter()
        .flat_map(|service| environment(compose, service).into_keys())
        .collect()
}

/// The variables a compose file reads from its env file: `${NAME…}`.
fn interpolated(compose: &str) -> BTreeSet<String> {
    let reference = Regex::new(r"\$\{([A-Z][A-Z0-9_]*)").unwrap();
    reference
        .captures_iter(compose)
        .map(|found| found[1].to_string())
        .collect()
}

fn names(exceptions: &[(&str, &str)]) -> BTreeSet<String> {
    exceptions
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

fn missing(expected: &BTreeSet<String>, found: &BTreeSet<String>) -> Vec<String> {
    expected.difference(found).cloned().collect()
}

#[test]
fn the_configuration_reads_what_the_tests_record() {
    let read = read_by_the_configuration();
    // A sample of each branch: none is missed by the recording.
    for name in [
        "DATABASE_APP_URL",
        "MIMORYA_MASTER_KEY",
        "ANTHROPIC_BASE_URL",
        "MIMORYA_OUTBOUND_NO_PROXY",
        "MAIL_FROM",
        "MIMORYA_DECISION_TIMEOUT_MS",
    ] {
        assert!(read.contains(name), "{name} not recorded: {read:?}");
    }
    // Every exception is a variable still read, its reason still owed.
    for (name, _) in FIXED_BY_THE_STACK.iter().chain(&HELD_BY_THE_IMAGE) {
        assert!(read.contains(*name), "{name} is no longer read: drop it");
    }
}

#[test]
fn the_development_example_describes_every_variable_read() {
    let read = read_by_the_configuration();
    let described = described(&repository(".env.example"));
    assert_eq!(
        missing(&read, &described),
        Vec::<String>::new(),
        ".env.example"
    );
}

#[test]
fn the_production_example_describes_every_variable_the_operator_sets() {
    let mut expected = read_by_the_configuration();
    for name in names(&FIXED_BY_THE_STACK).union(&names(&HELD_BY_THE_IMAGE)) {
        expected.remove(name);
    }
    let described = described(&repository(".env.production.example"));
    assert_eq!(
        missing(&expected, &described),
        Vec::<String>::new(),
        ".env.production.example"
    );
    // What the stack fixes or the image holds is never offered to set.
    for name in names(&FIXED_BY_THE_STACK).union(&names(&HELD_BY_THE_IMAGE)) {
        assert!(
            !described.contains(name),
            "{name} is not the operator's to set"
        );
    }
}

#[test]
fn the_production_stack_passes_every_variable_read() {
    let mut expected = read_by_the_configuration();
    for name in names(&HELD_BY_THE_IMAGE) {
        expected.remove(&name);
    }
    let given = given(&repository("compose.production.yml"));
    assert_eq!(
        missing(&expected, &given),
        Vec::<String>::new(),
        "compose.production.yml"
    );
}

#[test]
fn the_production_example_and_the_stack_name_the_same_variables() {
    let described = described(&repository(".env.production.example"));
    let interpolated = interpolated(&repository("compose.production.yml"));
    assert_eq!(
        missing(&described, &interpolated),
        Vec::<String>::new(),
        "described in .env.production.example, never read by compose.production.yml"
    );
    assert_eq!(
        missing(&interpolated, &described),
        Vec::<String>::new(),
        "read by compose.production.yml, never described in .env.production.example"
    );
}

#[test]
fn the_installation_guide_names_every_variable_to_set() {
    // The required and recommended parts of the example, before the
    // optional one: what an installation sets (docs/INSTALLATION.md).
    let example = repository(".env.production.example");
    let (to_set, _) = example
        .split_once("3. FACULTATIVES")
        .expect("the example's optional part");
    let guide = repository("docs/INSTALLATION.md");
    let unnamed: Vec<String> = stack::set_in(to_set)
        .into_keys()
        .filter(|name| !guide.contains(&format!("`{name}`")))
        .collect();
    assert_eq!(unnamed, Vec::<String>::new(), "docs/INSTALLATION.md");
}

#[test]
fn the_guide_and_the_acceptance_tests_name_commands_that_exist() {
    let usage = repository("crates/api/src/bin/admin/usage.rs");
    let command = Regex::new(r"mimorya-admin ([a-z][a-z-]*)").unwrap();
    let script = Regex::new(r"infra/backup/([a-z][a-z-]*\.sh)").unwrap();
    for document in ["docs/INSTALLATION.md", "docs/RECETTE_BETA.md"] {
        let text = repository(document);
        for found in command.captures_iter(&text) {
            let listed = usage
                .lines()
                .any(|line| line.trim_start().split(' ').next() == Some(&found[1]));
            assert!(
                listed,
                "{document}: mimorya-admin {} is no command",
                &found[1]
            );
        }
        for found in script.captures_iter(&text) {
            let path = format!("infra/backup/{}", &found[1]);
            assert!(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(&path)
                    .is_file(),
                "{document}: {path} does not exist"
            );
        }
    }
}
