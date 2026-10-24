//! Documents a company deposits for its agent to read (ADR-095): which files
//! are taken and how they are recognised (`kinds`), the limits their
//! reading keeps to (`limits`), what reading one gives or why it is refused
//! (`reading`), and the passages its text is cut into (`chunking`). Pure:
//! the file is read by a process of its own (`mimorya-extract`).

mod chunking;
mod kinds;
pub mod limits;
mod reading;

pub use chunking::{PASSAGE_MAX_CHARS, PASSAGE_TARGET_CHARS, Passage, passages};
pub use kinds::DocumentKind;
pub use reading::{Extracted, Reading, Refusal};
