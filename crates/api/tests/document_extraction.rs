//! The extraction process (ADR-095), started as the API starts it: each
//! kind of file read through it, a refused one with its reason. A file that
//! exhausts its memory stops that process only. One that hangs is killed at
//! its time, one that dies, floods its output or answers nonsense is
//! refused, and none sees the API's environment. At most
//! `CONCURRENT_EXTRACTIONS` read at once.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use mimorya_api::services::documents::{CONCURRENT_EXTRACTIONS, ExtractionFailure, Extractor};
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use mimorya_core::documents::{DocumentKind, Refusal};
use mimorya_extract::testing::{docx, inflating_pdf, pdf, protected_pdf};
use uuid::Uuid;

const PROGRAM: &str = env!("CARGO_BIN_EXE_mimorya-extract");

fn extractor() -> Extractor {
    Extractor::new(PROGRAM.into())
}

/// A program standing in for the reader: `script`, run by the shell.
fn impostor(script: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("mimorya-extract-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let program = directory.join("mimorya-extract");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    program
}

#[tokio::test]
async fn each_kind_is_read_by_the_extraction_process() {
    let reader = extractor();
    let read = reader
        .extract(DocumentKind::Pdf, pdf(&["Première page.", "Seconde page."]))
        .await
        .unwrap();
    assert!(read.paged);
    assert_eq!(read.pages, ["Première page.", "Seconde page."]);
    let paragraph = "<w:p><w:r><w:t>Procédure de purge.</w:t></w:r></w:p>";
    let read = reader
        .extract(DocumentKind::Docx, docx(paragraph))
        .await
        .unwrap();
    assert_eq!(read.pages, ["Procédure de purge."]);
    for kind in [DocumentKind::Markdown, DocumentKind::Text] {
        let read = reader
            .extract(kind, b"# Note\n\nTexte.".to_vec())
            .await
            .unwrap();
        assert_eq!(read.pages, ["# Note\n\nTexte."]);
    }
}

#[tokio::test]
async fn a_file_the_reader_refuses_says_why() {
    let reader = extractor();
    let locked = protected_pdf(&["Secret."], "motdepasse");
    let refused = reader.extract(DocumentKind::Pdf, locked).await;
    assert!(
        matches!(refused, Err(ExtractionFailure::Refused(Refusal::Encrypted))),
        "{refused:?}"
    );
    let latin1 = b"caf\xe9".to_vec();
    let refused = reader.extract(DocumentKind::Text, latin1).await;
    assert!(
        matches!(refused, Err(ExtractionFailure::Refused(Refusal::NotUtf8))),
        "{refused:?}"
    );
    let large = vec![b'a'; MAX_FILE_BYTES + 1];
    let refused = reader.extract(DocumentKind::Text, large).await;
    assert!(
        matches!(refused, Err(ExtractionFailure::Refused(Refusal::TooLarge))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_file_beyond_the_reader_s_memory_stops_that_process_only() {
    let reader = extractor();
    let bomb = inflating_pdf(1300 * 1024 * 1024);
    let stopped = reader.extract(DocumentKind::Pdf, bomb).await;
    assert!(
        matches!(
            stopped,
            Err(ExtractionFailure::Failed(_) | ExtractionFailure::Refused(Refusal::Unreadable))
        ),
        "{stopped:?}"
    );
    // This process goes on reading.
    let read = reader
        .extract(DocumentKind::Text, b"Suite.".to_vec())
        .await
        .unwrap();
    assert_eq!(read.pages, ["Suite."]);
}

#[tokio::test]
async fn a_reader_that_hangs_is_killed_at_its_time() {
    let reader =
        Extractor::with_timeout(impostor("exec /bin/sleep 30"), Duration::from_millis(300));
    let started = Instant::now();
    let stopped = reader.extract(DocumentKind::Text, b"x".to_vec()).await;
    assert!(
        matches!(stopped, Err(ExtractionFailure::TimedOut)),
        "{stopped:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_reader_that_dies_floods_or_answers_nonsense_is_refused() {
    for script in [
        "exit 3",
        "exec /usr/bin/yes",
        "echo nonsense",
        "echo '{\"outcome\":\"read\"}'",
    ] {
        let reader = Extractor::new(impostor(script));
        let refused = reader.extract(DocumentKind::Text, b"x".to_vec()).await;
        assert!(
            matches!(refused, Err(ExtractionFailure::Failed(_))),
            "{script}: {refused:?}"
        );
    }
    let missing = Extractor::new(PathBuf::from("/nonexistent/mimorya-extract"));
    let refused = missing.extract(DocumentKind::Text, b"x".to_vec()).await;
    assert!(
        matches!(refused, Err(ExtractionFailure::NotStarted(_))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn the_reader_sees_none_of_the_api_s_environment() {
    // Set by cargo for the tests, as a secret would be for the API.
    assert!(std::env::var("CARGO_PKG_NAME").is_ok());
    let echo = "printf '{\"outcome\":\"read\",\"pages\":[\"%s\"],\"paged\":false}' \"${CARGO_PKG_NAME-vide}\"";
    let reader = Extractor::new(impostor(echo));
    let read = reader
        .extract(DocumentKind::Text, b"x".to_vec())
        .await
        .unwrap();
    assert_eq!(read.pages, ["vide"]);
}

#[tokio::test]
async fn no_more_files_than_allowed_are_read_at_once() {
    let reader = Extractor::with_timeout(impostor("exec /bin/sleep 30"), Duration::from_secs(2));
    let mut reading = Vec::new();
    for _ in 0..CONCURRENT_EXTRACTIONS {
        let reader = reader.clone();
        reading.push(tokio::spawn(async move {
            reader.extract(DocumentKind::Text, b"x".to_vec()).await
        }));
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let busy = reader.extract(DocumentKind::Text, b"x".to_vec()).await;
    assert!(matches!(busy, Err(ExtractionFailure::Busy)), "{busy:?}");
    for read in reading {
        assert!(matches!(
            read.await.unwrap(),
            Err(ExtractionFailure::TimedOut)
        ));
    }
}
