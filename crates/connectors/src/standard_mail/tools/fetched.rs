//! E-mails fetched from the IMAP server (ADR-205): only those asked for
//! are kept, however many the server sends back, and no more than asked.
//! A server may send what it was not asked (another e-mail's changes): it is
//! read, within the session's budget, and left.

use async_imap::error::Result as ImapResult;
use async_imap::types::{Fetch, Flag};
use futures::{Stream, StreamExt};

use super::person::failure;
use crate::outbound::{Cause, ReadBudget};

/// What `stream` sent of the e-mails `wanted` takes, `max` at most.
pub async fn fetched(
    stream: impl Stream<Item = ImapResult<Fetch>>,
    budget: &ReadBudget,
    wanted: impl Fn(&Fetch) -> bool,
    max: usize,
) -> Result<Vec<Fetch>, Cause> {
    let mut stream = Box::pin(stream);
    let mut kept = Vec::new();
    while let Some(fetch) = stream.next().await {
        let fetch = fetch.map_err(|error| failure(budget, error))?;
        if kept.len() < max && fetch.uid.is_some() && wanted(&fetch) {
            kept.push(fetch);
        }
    }
    Ok(kept)
}

/// Whether the person read the e-mail.
pub fn seen(fetch: &Fetch) -> bool {
    fetch.flags().any(|flag| matches!(flag, Flag::Seen))
}
