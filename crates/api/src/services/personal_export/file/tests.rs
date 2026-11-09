//! An export file's place (ADR-187, ADR-188): held while the file is
//! unread, given back once it is read to its end, dropped, or left with no
//! block taken for its stall, its sending then ended in error and the file
//! let go of, though a block taken is still held.

use std::time::Duration;

use axum::body::Bytes;
use futures::StreamExt;
use uuid::Uuid;

use super::{BLOCK, ExportFile};
use crate::services::places::Places;

const STALL: Duration = Duration::from_secs(5);
const PERSON: Uuid = Uuid::nil();

fn places() -> Places {
    Places::new(1, "occupé", "déjà en cours")
}

/// A file of `size` bytes holding `places`' one place.
fn file(places: &Places, size: usize) -> ExportFile {
    ExportFile {
        day: "2026-11-08".into(),
        bytes: Bytes::from(vec![b'x'; size]),
        place: places.take(PERSON).unwrap(),
    }
}

#[tokio::test(start_paused = true)]
async fn a_file_read_to_its_end_gives_its_place_back() {
    let places = places();
    let mut blocks = file(&places, BLOCK * 2 + 1)
        .into_body_within(STALL)
        .into_data_stream();
    assert_eq!(blocks.next().await.unwrap().unwrap().len(), BLOCK);
    assert!(places.take(PERSON).is_err());
    assert_eq!(blocks.next().await.unwrap().unwrap().len(), BLOCK);
    assert_eq!(blocks.next().await.unwrap().unwrap().len(), 1);
    assert!(places.take(PERSON).is_err());
    assert!(blocks.next().await.is_none());
    assert!(places.take(PERSON).is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_file_dropped_unread_gives_its_place_back() {
    let places = places();
    let body = file(&places, 10).into_body_within(STALL);
    assert!(places.take(PERSON).is_err());
    drop(body);
    assert!(places.take(PERSON).is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_file_whose_blocks_are_no_longer_taken_lets_its_place_go() {
    let places = places();
    let mut blocks = file(&places, BLOCK * 3)
        .into_body_within(STALL)
        .into_data_stream();
    assert!(blocks.next().await.unwrap().is_ok());

    // A block taken just before the stall: the sending goes on.
    tokio::time::sleep(STALL - Duration::from_secs(1)).await;
    assert!(places.take(PERSON).is_err());
    assert!(blocks.next().await.unwrap().is_ok());
    tokio::time::sleep(STALL - Duration::from_secs(1)).await;
    assert!(places.take(PERSON).is_err());

    // Then none for the stall: the place is given back, the sending ends
    // in error, and nothing follows.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(places.take(PERSON).is_ok());
    assert!(blocks.next().await.unwrap().is_err());
    assert!(blocks.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn a_stalled_file_is_let_go_of_though_its_blocks_are_still_held() {
    let places = places();
    let written = Bytes::from(vec![b'x'; BLOCK * 3]);
    let file = ExportFile {
        day: "2026-11-08".into(),
        bytes: written.clone(),
        place: places.take(PERSON).unwrap(),
    };
    let mut blocks = file.into_body_within(STALL).into_data_stream();
    // A block still held, as by a server whose client no longer reads.
    let held = blocks.next().await.unwrap().unwrap();
    assert!(!written.is_unique());

    tokio::time::sleep(STALL + Duration::from_secs(1)).await;
    assert!(places.take(PERSON).is_ok());
    // The file is let go of: the block held is a copy of it.
    assert!(written.is_unique());
    assert_eq!(held.len(), BLOCK);
}
