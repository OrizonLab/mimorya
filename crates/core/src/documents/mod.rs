//! Documents a company deposits for its agent to read (ADR-095): which files
//! are taken and how they are recognised (`kinds`), their name and title
//! (`names`), the limits their reading keeps to (`limits`), what reading one
//! gives or why it is refused (`reading`), the passages its text is cut into
//! (`chunking`), and who reads or writes where (`spaces`). Pure: the file is
//! read by a process of its own (`mimorya-extract`).

mod chunking;
mod kinds;
pub mod limits;
mod names;
mod reading;
mod spaces;

pub use chunking::{PASSAGE_MAX_CHARS, PASSAGE_TARGET_CHARS, Passage, passages};
pub use kinds::DocumentKind;
pub use names::{MAX_FILE_NAME_CHARS, MAX_TITLE_CHARS, file_name, title_of};
pub use reading::{Extracted, Reading, Refusal};
pub use spaces::{
    DocumentReadFilter, DocumentSpace, can_write_document, document_read_filter, writable_spaces,
};
