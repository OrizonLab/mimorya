//! What the simulated SMTP server takes (ADR-205): the e-mails handed to it
//! once an account signed in, each with its envelope and its text as
//! received, dots unstuffed; and what it does once a text is handed: take
//! it, refuse it, or leave without a word.

use std::sync::{Arc, Mutex};

use tokio::io::AsyncBufRead;

use super::lines::next_line;

/// The most of a text the server reads.
const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// What the server does once a text is handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterData {
    /// Takes it (`250`).
    Takes,
    /// Refuses it (`554`).
    Refuses,
    /// Refuses it for now (`451`).
    RefusesForNow,
    /// Closes the connection without answering.
    Drops,
    /// Never answers, the connection kept open.
    Hangs,
}

/// An e-mail the server took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub from: String,
    pub to: Vec<String>,
    pub text: String,
}

/// The e-mails the server took, in order.
#[derive(Default, Clone)]
pub struct Deliveries(Arc<Mutex<Vec<Delivery>>>);

impl Deliveries {
    pub fn all(&self) -> Vec<Delivery> {
        self.0.lock().expect("deliveries").clone()
    }

    pub(super) fn add(&self, delivery: Delivery) {
        self.0.lock().expect("deliveries").push(delivery);
    }
}

/// The envelope a client gives, before its text.
#[derive(Default)]
pub(super) struct Envelope {
    pub from: Option<String>,
    pub to: Vec<String>,
}

/// The address of `FROM:<…>` or `TO:<…>`, `keyword` named.
pub(super) fn path(argument: &str, keyword: &str) -> Option<String> {
    let (named, rest) = argument.split_once(':')?;
    if !named.trim().eq_ignore_ascii_case(keyword) {
        return None;
    }
    let inner = rest.trim().strip_prefix('<')?;
    let (address, _) = inner.split_once('>')?;
    Some(address.to_string())
}

/// The text up to its lone dot, each line's leading dot unstuffed; `None`
/// once the client is gone or the text is longer than the server reads.
pub(super) async fn data_text<R: AsyncBufRead + Unpin>(reader: &mut R) -> Option<String> {
    let mut text = String::new();
    loop {
        let line = next_line(reader).await?;
        if line == "." {
            return Some(text);
        }
        let line = line.strip_prefix('.').unwrap_or(&line);
        text.push_str(line);
        text.push_str("\r\n");
        if text.len() > MAX_TEXT_BYTES {
            return None;
        }
    }
}
