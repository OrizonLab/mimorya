//! Documents a company deposits for its agent to read (ADR-095): their
//! text read by the extraction process (`extractor`), deposited under the
//! company's quota (`deposit`), listed, downloaded and deleted (`library`),
//! each where the person's rights reach only.

mod deposit;
mod extractor;
mod library;

use std::sync::Arc;

use mimorya_db::Database;

pub use deposit::{Deposit, DepositFailure};
pub use extractor::{
    CONCURRENT_EXTRACTIONS, EXTRACTION_TIMEOUT, ExtractionFailure, Extractor, extractor_beside,
};
pub use library::DocumentFile;

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
