//! The agent reads nothing from the process's environment (ADR-221): a
//! provider takes its key and its address from the configuration the API
//! reads, where an empty value counts as none, so that no provider can pick
//! up the operator's gateway or key by itself.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_source_of_the_agent_reads_the_environment() {
    let mut found = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(found.len() > 50, "only {} sources found", found.len());
    let mut reading = Vec::new();
    for path in &found {
        let text = fs::read_to_string(path).unwrap();
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("std::env") || code.contains("env::var") {
                reading.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }
    assert!(reading.is_empty(), "the environment is read at {reading:?}");
}
