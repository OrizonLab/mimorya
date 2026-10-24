//! The extraction process: `mimorya-extract <kind>`, the file on its
//! standard input, its reading (`Reading`) on its standard output, as JSON.
//! Before reading anything it confines itself: memory, processor time, no
//! file written, few descriptors, no core dump. The API starts it with an
//! empty environment, waits a bounded time and reads a bounded answer
//! (`services/documents/extractor.rs`); one that dies or hangs is refused
//! there. Nothing else is read or written.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use mimorya_core::documents::limits::MAX_FILE_BYTES;
use mimorya_core::documents::{DocumentKind, Reading, Refusal};

/// The memory the process may map.
pub const MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
/// The processor time it may use.
pub const CPU_SECONDS: u64 = 20;
/// The descriptors it may hold open.
const OPEN_FILES: u64 = 16;

/// Exit codes: the kind argument missing or unknown, the confinement or
/// the standard input or output failing.
const BAD_ARGUMENT: u8 = 2;
const NOT_CONFINED: u8 = 3;
const NO_INPUT: u8 = 4;
const NO_OUTPUT: u8 = 5;

pub fn run() -> ExitCode {
    let Some(kind) = std::env::args()
        .nth(1)
        .and_then(|kind| DocumentKind::parse(&kind))
    else {
        return ExitCode::from(BAD_ARGUMENT);
    };
    if confine().is_err() {
        return ExitCode::from(NOT_CONFINED);
    }
    let mut bytes = Vec::new();
    let limit = MAX_FILE_BYTES as u64 + 1;
    if io::stdin()
        .lock()
        .take(limit)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return ExitCode::from(NO_INPUT);
    }
    let reading = if bytes.len() > MAX_FILE_BYTES {
        Reading::Refused {
            reason: Refusal::TooLarge,
        }
    } else {
        Reading::from(crate::read(kind, &bytes))
    };
    let mut output = io::stdout().lock();
    if serde_json::to_writer(&mut output, &reading).is_err() || output.flush().is_err() {
        return ExitCode::from(NO_OUTPUT);
    }
    ExitCode::SUCCESS
}

#[cfg(unix)]
fn confine() -> io::Result<()> {
    use rlimit::Resource;
    limit(Resource::CORE, 0)?;
    limit(Resource::CPU, CPU_SECONDS)?;
    limit(Resource::FSIZE, 0)?;
    limit(Resource::NOFILE, OPEN_FILES)?;
    limit(Resource::AS, MEMORY_BYTES)
}

/// Sets `resource`'s limits, soft and hard, to `value`, or to the hard
/// limit already in force if lower: this process may only lower it.
#[cfg(unix)]
fn limit(resource: rlimit::Resource, value: u64) -> io::Result<()> {
    let (_, hard) = resource.get()?;
    let value = value.min(hard);
    resource.set(value, value)
}

#[cfg(not(unix))]
fn confine() -> io::Result<()> {
    Err(io::Error::other("no confinement on this system"))
}
