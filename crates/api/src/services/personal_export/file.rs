//! The file of a person's export (ADR-187): the JSON written once, then
//! sent a block at a time by a body that holds the export's place. The
//! place is given back when the last block is taken or the body is dropped
//! (the person gone before the end), never before: axum writes a response
//! after its handler returns, so a place given back with the handler would
//! let a process write more files at once than it may.

use std::convert::Infallible;

use axum::body::{Body, Bytes};
use futures::stream;

use crate::contract::PersonalExport;
use crate::services::places::Place;

/// The bytes sent at a time.
const BLOCK: usize = 64 * 1024;

/// An export, written, and its place.
pub struct ExportFile {
    /// The day it was made, `YYYY-MM-DD`, for the file's name.
    pub day: String,
    bytes: Bytes,
    place: Place,
}

impl ExportFile {
    pub(super) fn new(export: &PersonalExport, place: Place) -> anyhow::Result<Self> {
        let day = export.exported_at.get(..10).unwrap_or("export").to_owned();
        let bytes = Bytes::from(serde_json::to_vec(export)?);
        Ok(Self { day, bytes, place })
    }

    /// The body that sends the file, holding its place until the end.
    pub fn into_body(self) -> Body {
        let blocks = stream::unfold((self.bytes, self.place), |(mut rest, place)| async move {
            if rest.is_empty() {
                drop(place);
                return None;
            }
            let block = rest.split_to(rest.len().min(BLOCK));
            Some((Ok::<_, Infallible>(block), (rest, place)))
        });
        Body::from_stream(blocks)
    }
}
