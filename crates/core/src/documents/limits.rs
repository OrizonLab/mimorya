//! What a deposited file may hold (ADR-095): beyond, it is refused before
//! any of it is kept. They bound what reading a hostile file may cost.

/// A file's size, as deposited.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
/// A PDF's pages.
pub const MAX_PAGES: usize = 500;
/// The text read from a file, all pages together.
pub const MAX_TEXT_CHARS: usize = 1_000_000;
/// The entries of a Word archive (.docx).
pub const MAX_ARCHIVE_ENTRIES: usize = 2_000;
/// The text part of a Word archive, once uncompressed.
pub const MAX_ARCHIVE_PART_BYTES: u64 = 50 * 1024 * 1024;
/// How much larger than compressed that part may be: beyond, a
/// decompression bomb.
pub const MAX_COMPRESSION_RATIO: u64 = 100;
