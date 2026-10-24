//! Reads a deposited file's text through the extraction process (ADR-095):
//! `mimorya-extract <kind>`, started with an empty environment, the file on
//! its input, its reading on its output. Hostile input is parsed there, not
//! here: a file that crashes it, loops or exhausts its memory loses that
//! process only. This side bounds the rest: `CONCURRENT_EXTRACTIONS` at a
//! time in this process, `EXTRACTION_TIMEOUT` each, then killed, and an
//! answer read up to `MAX_ANSWER_BYTES`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use mimorya_core::documents::{DocumentKind, Extracted, Reading, Refusal};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Semaphore;

/// Files read at once by this process.
pub const CONCURRENT_EXTRACTIONS: usize = 2;
/// How long a file's reading may take, the process's start included.
pub const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest answer read: the longest text, as JSON.
const MAX_ANSWER_BYTES: u64 = 8 * 1024 * 1024;
/// The program's name, beside the API's.
const PROGRAM: &str = "mimorya-extract";

/// Why a file's text was not read.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionFailure {
    /// The file, as the reader found it.
    #[error("file refused: {0:?}")]
    Refused(Refusal),
    /// `CONCURRENT_EXTRACTIONS` files are being read.
    #[error("too many files being read")]
    Busy,
    /// The reader took longer than its time, and was killed.
    #[error("reading timed out")]
    TimedOut,
    /// The reader died, or answered beyond its bound or not as expected:
    /// its limits stopped it, or the file broke it.
    #[error("reader failed: {0}")]
    Failed(&'static str),
    /// The reader could not be started: its program is missing.
    #[error("reader not started: {0}")]
    NotStarted(std::io::Error),
}

#[derive(Debug, Clone)]
pub struct Extractor {
    program: PathBuf,
    permits: Arc<Semaphore>,
    timeout: Duration,
}

/// The extraction program beside the running one, as installed.
pub fn extractor_beside(running: &Path) -> PathBuf {
    running.with_file_name(PROGRAM)
}

impl Extractor {
    pub fn new(program: PathBuf) -> Self {
        Self::with_timeout(program, EXTRACTION_TIMEOUT)
    }

    /// One whose readings stop after `timeout`.
    pub fn with_timeout(program: PathBuf, timeout: Duration) -> Self {
        Self {
            program,
            permits: Arc::new(Semaphore::new(CONCURRENT_EXTRACTIONS)),
            timeout,
        }
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The text of `bytes`, a file of `kind`.
    pub async fn extract(
        &self,
        kind: DocumentKind,
        bytes: Vec<u8>,
    ) -> Result<Extracted, ExtractionFailure> {
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ExtractionFailure::Busy)?;
        let mut child = Command::new(&self.program)
            .arg(kind.as_str())
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(ExtractionFailure::NotStarted)?;
        let answered = tokio::time::timeout(self.timeout, answer(&mut child, bytes)).await;
        let answer = match answered {
            Ok(answer) => answer,
            Err(_) => {
                stop(&mut child).await;
                return Err(ExtractionFailure::TimedOut);
            }
        };
        let answer = match answer {
            Ok(answer) => answer,
            Err(failure) => {
                stop(&mut child).await;
                return Err(failure);
            }
        };
        match serde_json::from_slice::<Reading>(&answer) {
            Ok(Reading::Read(extracted)) => Ok(extracted),
            Ok(Reading::Refused { reason }) => Err(ExtractionFailure::Refused(reason)),
            Err(_) => Err(ExtractionFailure::Failed("unexpected answer")),
        }
    }
}

/// Gives the reader `bytes`, then reads its answer until it exits.
async fn answer(child: &mut Child, bytes: Vec<u8>) -> Result<Vec<u8>, ExtractionFailure> {
    let mut input = child
        .stdin
        .take()
        .ok_or(ExtractionFailure::Failed("no input"))?;
    // The reader may stop reading early (a file too large): what it did not
    // take is not needed.
    let feeding = tokio::spawn(async move {
        let _ = input.write_all(&bytes).await;
    });
    let output = child
        .stdout
        .take()
        .ok_or(ExtractionFailure::Failed("no output"))?;
    let mut answer = Vec::new();
    output
        .take(MAX_ANSWER_BYTES + 1)
        .read_to_end(&mut answer)
        .await
        .map_err(|_| ExtractionFailure::Failed("output unreadable"))?;
    if answer.len() as u64 > MAX_ANSWER_BYTES {
        feeding.abort();
        return Err(ExtractionFailure::Failed("answer too long"));
    }
    let status = child
        .wait()
        .await
        .map_err(|_| ExtractionFailure::Failed("not awaited"))?;
    feeding.abort();
    if !status.success() {
        return Err(ExtractionFailure::Failed("exited in failure"));
    }
    Ok(answer)
}

/// Kills the reader and waits for its end, so that it leaves no zombie.
async fn stop(child: &mut Child) {
    let _ = child.kill().await;
}
