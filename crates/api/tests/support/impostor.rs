//! A program standing in for the extraction process (ADR-095): a shell
//! script that hangs, dies or answers as a test needs, removed with its
//! directory once dropped.

use std::path::PathBuf;
use std::process::Command;

use mimorya_api::services::documents::Extractor;
use uuid::Uuid;

pub struct Impostor {
    directory: PathBuf,
}

impl Impostor {
    /// `script`, run by the shell. It is written by a shell of its own: had
    /// this process opened it to write, a process another test starts
    /// meanwhile could inherit that descriptor, and running the script
    /// would then fail (« Text file busy »).
    pub fn new(script: &str) -> Self {
        let directory = std::env::temp_dir().join(format!("mimorya-extract-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let impostor = Self { directory };
        let written = Command::new("/bin/sh")
            .args([
                "-c",
                r#"printf '#!/bin/sh\n%s\n' "$1" > "$2" && chmod 700 "$2""#,
            ])
            .args(["sh", script])
            .arg(impostor.program())
            .status()
            .unwrap();
        assert!(written.success());
        impostor
    }

    pub fn program(&self) -> PathBuf {
        self.directory.join("mimorya-extract")
    }

    pub fn reader(&self) -> Extractor {
        Extractor::new(self.program())
    }
}

impl Drop for Impostor {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
