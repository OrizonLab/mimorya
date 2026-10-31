//! Documents a company deposits for its agent to read (ADR-095): their
//! text read by the extraction process (`extractor`, `reading`), deposited
//! under the company's quota (`deposit`), replaced by a new version of
//! their file (`replace`, ADR-150), listed, downloaded and deleted
//! (`library`), each where the person's rights reach only.

mod deposit;
mod extractor;
mod library;
mod reading;
mod replace;

use std::sync::Arc;

use mimorya_db::Database;

pub use deposit::Deposit;
pub use extractor::{
    CONCURRENT_EXTRACTIONS, EXTRACTION_TIMEOUT, ExtractionFailure, Extractor, extractor_beside,
};
pub use library::{DocumentFile, LISTED};
pub use reading::FileFailure;
pub use replace::Replacement;

use super::embeddings::EmbeddingService;

pub struct DocumentService {
    db: Database,
    extractor: Extractor,
    /// The bytes of documents a company may keep.
    quota_bytes: i64,
    /// Told of each deposit: its passages await their vectors (ADR-096).
    embeddings: Arc<EmbeddingService>,
}

impl DocumentService {
    pub fn new(
        db: Database,
        extractor: Extractor,
        quota_bytes: i64,
        embeddings: Arc<EmbeddingService>,
    ) -> Self {
        Self {
            db,
            extractor,
            quota_bytes,
            embeddings,
        }
    }
}
