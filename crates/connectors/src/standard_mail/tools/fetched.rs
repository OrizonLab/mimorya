//! E-mails fetched from the IMAP server (ADR-205): only those asked for
//! are kept, with the part asked for, once each, however many the server
//! sends back, and no more than asked. A server may send what it was not
//! asked (another e-mail's changes, or the flags of one asked for, read
//! meanwhile elsewhere): it is read, within the session's budget, and left.

use std::collections::HashSet;

use async_imap::error::Result as ImapResult;
use async_imap::types::{Fetch, Flag};
use futures::{Stream, StreamExt};

use super::person::failure;
use crate::outbound::{Cause, ReadBudget};

/// What `stream` sent of the e-mails `wanted` takes, the first of each UID,
/// `max` at most.
pub async fn fetched(
    stream: impl Stream<Item = ImapResult<Fetch>>,
    budget: &ReadBudget,
    wanted: impl Fn(&Fetch) -> bool,
    max: usize,
) -> Result<Vec<Fetch>, Cause> {
    let mut stream = Box::pin(stream);
    let mut kept = Vec::new();
    let mut seen = HashSet::new();
    while let Some(fetch) = stream.next().await {
        let fetch = fetch.map_err(|error| failure(budget, error))?;
        let Some(uid) = fetch.uid else { continue };
        if kept.len() < max && wanted(&fetch) && seen.insert(uid) {
            kept.push(fetch);
        }
    }
    Ok(kept)
}

/// Whether the person read the e-mail.
pub fn seen(fetch: &Fetch) -> bool {
    fetch.flags().any(|flag| matches!(flag, Flag::Seen))
}
