//! What the simulated SMTP server may do on hearing a command instead of
//! answering it at once (ADR-205): leave, fall silent, or answer at a
//! length that runs a client's reading budget out over a few answers.

/// What the server does instead of answering a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instead {
    /// Closes the connection.
    Drops,
    /// Never answers, the connection kept open.
    Hangs,
    /// Answers after 90 kB of continuation lines, each line within what a
    /// client reads of one, the whole within what it reads of one answer.
    AnswersAtLength,
}

/// Continuation lines before an answer at length.
const LINES: usize = 100;
/// The characters of each, after `250-`.
const LINE_CHARS: usize = 896;

/// `answer`, after the continuation lines of an answer at length.
pub(super) fn lengthened(answer: &str) -> String {
    let line = format!("250-{}\r\n", "a".repeat(LINE_CHARS));
    let mut text = line.repeat(LINES);
    text.push_str(answer);
    text
}
