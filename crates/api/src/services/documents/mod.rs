//! Documents a company deposits for its agent to read (ADR-095): their
//! text read by the extraction process (`extractor`), deposited under the
//! company's quota (`deposit`), listed, downloaded and deleted (`library`),
//! each where the person's rights reach only.

mod deposit;
mod extractor;
mod library;

use mimorya_db::Database;

pub use deposit::{Deposit, DepositFailure};
pub use extractor::{
    CONCURRENT_EXTRACTIONS, EXTRACTION_TIMEOUT, ExtractionFailure, Extractor, extractor_beside,
};
pub use library::DocumentFile;

pub struct DocumentService {
    db: Database,
    extractor: Extractor,
    /// The bytes of documents a company may keep.
    quota_bytes: i64,
}

impl DocumentService {
    pub fn new(db: Database, extractor: Extractor, quota_bytes: i64) -> Self {
        Self {
            db,
            extractor,
            quota_bytes,
        }
    }
}
