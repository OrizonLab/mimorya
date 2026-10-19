//! A run's rights as tests see them (ADR-035): fixed, or changing from one
//! step to the next, as when an administrator withdraws a right or disables
//! the account while the run goes on.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mimorya_core::authz::{AuthorizationEngine, Subject};

use crate::runtime::{AccessSource, CurrentAccess};

pub struct TestAccess {
    /// What each successive read returns; the last one stays.
    steps: Mutex<Vec<Option<CurrentAccess>>>,
}

impl TestAccess {
    /// The same rights for the whole run.
    pub fn fixed(engine: Arc<AuthorizationEngine>, subject: Arc<Subject>) -> Arc<Self> {
        Self::sequence(vec![Some(CurrentAccess { engine, subject })])
    }

    /// One answer per read (`None`: the account is no longer active), the
    /// last repeated. An empty sequence reads as an inactive account.
    pub fn sequence(steps: Vec<Option<CurrentAccess>>) -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(steps),
        })
    }
}

#[async_trait]
impl AccessSource for TestAccess {
    async fn current(&self) -> anyhow::Result<Option<CurrentAccess>> {
        let mut steps = self.steps.lock().expect("access steps");
        Ok(if steps.len() > 1 {
            steps.remove(0)
        } else {
            steps.first().cloned().flatten()
        })
    }
}
