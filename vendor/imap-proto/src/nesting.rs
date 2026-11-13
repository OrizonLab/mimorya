//! Patch of the Mimorya platform (ADR-205): how deep a response may nest
//! the parsers that call themselves, a body within a body or an extension
//! within an extension. Without it, the parser calls itself once for each
//! parenthesis a server sends and overflows the stack, which aborts the
//! whole process: past `MAX_NESTING` levels, the response is refused.

use std::cell::Cell;

use nom::error::{Error, ErrorKind};

/// The levels a response may nest: real messages nest a handful.
pub const MAX_NESTING: usize = 32;

thread_local! {
    static NESTING: Cell<usize> = const { Cell::new(0) };
}

/// One level entered, left when dropped.
pub(crate) struct Level(());

impl Level {
    /// Enters one level more at `i`; refused past `MAX_NESTING`, a failure
    /// no alternative recovers from.
    pub(crate) fn enter(i: &[u8]) -> Result<Level, nom::Err<Error<&[u8]>>> {
        NESTING.with(|nesting| {
            let level = nesting.get();
            if level >= MAX_NESTING {
                return Err(nom::Err::Failure(Error::new(i, ErrorKind::TooLarge)));
            }
            nesting.set(level + 1);
            Ok(Level(()))
        })
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        NESTING.with(|nesting| nesting.set(nesting.get().saturating_sub(1)));
    }
}
