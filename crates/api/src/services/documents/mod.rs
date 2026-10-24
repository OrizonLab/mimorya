//! Documents a company deposits for its agent to read (ADR-095): their
//! text read by the extraction process (`extractor`).

mod extractor;

pub use extractor::{
    CONCURRENT_EXTRACTIONS, EXTRACTION_TIMEOUT, ExtractionFailure, Extractor, extractor_beside,
};
