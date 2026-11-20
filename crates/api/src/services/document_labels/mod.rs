//! Deposited documents classified by the decision service (ADR-213, D4),
//! in the background: each document deposited or replaced while the
//! company's usage is not off awaits its classification
//! (`document_label_requests`); a pass, at startup, after each deposit or
//! replacement and every minute, asks the decision service its type and
//! its confidentiality on its title and first pages (`classify`), across
//! the companies (`pass`). Active, a stricter level is applied at once and
//! audited; a looser one and a type are proposed to a person who may write
//! the document, who applies or dismisses them (`services::documents`).
//! Observed, nothing changes. Failed, nothing changes either: the level the
//! depositor chose stays, which is the usage's prudent value.

mod classify;
mod pass;

use std::sync::Arc;

use mimorya_db::Database;
use tokio::sync::Notify;

use super::invalidations::{Invalidation, Invalidations};
use super::llm::LlmService;

pub struct DocumentLabelService {
    db: Database,
    llm: Arc<LlmService>,
    invalidations: Invalidations,
    /// Documents to classify: a pass is due.
    wake: Notify,
    /// One pass at a time in this process: a document is classified once.
    pass: tokio::sync::Mutex<()>,
}

impl DocumentLabelService {
    pub fn new(db: Database, llm: Arc<LlmService>, invalidations: Invalidations) -> Self {
        Self {
            db,
            llm,
            invalidations,
            wake: Notify::new(),
            pass: tokio::sync::Mutex::new(()),
        }
    }

    /// A document awaits its classification: a pass is due, whatever
    /// process classifies.
    pub fn wake(&self) {
        self.wake_here();
        self.invalidations.announce(Invalidation::DocumentLabels);
    }

    pub(crate) fn wake_here(&self) {
        self.wake.notify_one();
    }

    /// Returns once a pass is due (at most one wake is kept meanwhile).
    pub async fn woken(&self) {
        self.wake.notified().await;
    }
}
