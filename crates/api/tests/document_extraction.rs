//! The extraction process (ADR-095), started as the API starts it: each
//! kind of file read through it, a refused one with its reason. It confines
//! itself before reading (its limits read back from `/proc`), and a file
//! that exhausts its memory stops that process only. One that hangs is
//! killed at its time, one that dies, floods its output, answers nonsense
//! or beyond the limits is refused, and none sees the API's environment.
//! At most `CONCURRENT_EXTRACTIONS` read at once.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use mimorya_api::services::documents::{CONCURRENT_EXTRACTIONS, ExtractionFailure, Extractor};
use mimorya_core::documents::limits::{MAX_FILE_BYTES, MAX_PAGES};
use mimorya_core::documents::{DocumentKind, Refusal};
use mimorya_extract::testing::{docx, inflating_pdf, pdf, protected_pdf};
use uuid::Uuid;

const PROGRAM: &str = env!("CARGO_BIN_EXE_mimorya-extract");

fn extractor() -> Extractor {
    Extractor::new(PROGRAM.into())
}

/// A program standing in for the reader, removed with its directory once
/// dropped.
struct Impostor {
    directory: PathBuf,
}

impl Impostor {
    /// `script`, run by the shell. It is written by a shell of its own: had
    /// this process opened it to write, a process another test starts
    /// meanwhile could inherit that descriptor, and running the script
    /// would then fail (« Text file busy »).
    fn new(script: &str) -> Self {
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

    fn program(&self) -> PathBuf {
        self.directory.join("mimorya-extract")
    }

    fn reader(&self) -> Extractor {
        Extractor::new(self.program())
    }
}

impl Drop for Impostor {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
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
async fn the_reader_confines_itself_before_reading() {
    // Its input left open, it waits to read, its limits set.
    let mut reader = tokio::process::Command::new(PROGRAM)
        .arg("text")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let limits = Path::new("/proc")
        .join(reader.id().unwrap().to_string())
        .join("limits");
    let expected = [
        ("Max cpu time", "20"),
        ("Max file size", "0"),
        ("Max core file size", "0"),
        ("Max open files", "16"),
        ("Max address space", "1073741824"),
    ];
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let read = std::fs::read_to_string(&limits).unwrap();
        let set = expected.iter().all(|(name, value)| {
            read.lines().any(|line| {
                let Some(rest) = line.strip_prefix(name) else {
                    return false;
                };
                let columns: Vec<&str> = rest.split_whitespace().take(2).collect();
                columns == [*value, *value]
            })
        });
        if set {
            break;
        }
        assert!(Instant::now() < deadline, "limits not set:\n{read}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    reader.kill().await.unwrap();
}

#[tokio::test]
async fn a_file_beyond_the_reader_s_memory_stops_that_process_only() {
    let reader = extractor();
    let bomb = inflating_pdf(1300 * 1024 * 1024);
    let stopped = reader.extract(DocumentKind::Pdf, bomb).await;
    // Out of memory, the reader aborts: no answer, a failure.
    assert!(
        matches!(stopped, Err(ExtractionFailure::Failed(_))),
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
    let impostor = Impostor::new("exec /bin/sleep 30");
    let reader = Extractor::with_timeout(impostor.program(), Duration::from_millis(300));
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
    let pages = vec!["\"p\""; MAX_PAGES + 1].join(",");
    let beyond = format!("echo '{{\"outcome\":\"read\",\"pages\":[{pages}],\"paged\":true}}'");
    let scripts = [
        "exit 3",
        "exec /usr/bin/yes",
        "echo nonsense",
        "echo '{\"outcome\":\"read\"}'",
        // A text no reader would answer: empty, or beyond the pages.
        "echo '{\"outcome\":\"read\",\"pages\":[\" \"],\"paged\":false}'",
        &beyond,
    ];
    for script in scripts {
        let impostor = Impostor::new(script);
        let refused = impostor
            .reader()
            .extract(DocumentKind::Text, b"x".to_vec())
            .await;
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
    let impostor = Impostor::new(
        r#"printf '{"outcome":"read","pages":["%s"],"paged":false}' "${CARGO_PKG_NAME-vide}""#,
    );
    let read = impostor
        .reader()
        .extract(DocumentKind::Text, b"x".to_vec())
        .await
        .unwrap();
    assert_eq!(read.pages, ["vide"]);
}

#[tokio::test]
async fn no_more_files_than_allowed_are_read_at_once() {
    let impostor = Impostor::new("exec /bin/sleep 30");
    let reader = Extractor::with_timeout(impostor.program(), Duration::from_secs(2));
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
