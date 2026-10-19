//! A run's rights as tests see them (ADR-035): fixed, changing from one read
//! to the next (as when an administrator withdraws a right or disables the
//! account while the run goes on), or unreadable.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mimorya_core::authz::{AuthorizationEngine, Subject};

use crate::runtime::{ACCOUNT_INACTIVE, AccessSource, CurrentAccess, StepAccess};

enum Answer {
    Granted(CurrentAccess),
    Inactive,
    Fails,
}

pub struct TestAccess {
    /// What each successive read gives; the last one stays.
    answers: Mutex<Vec<Answer>>,
}

impl TestAccess {
    /// The same rights for the whole run.
    pub fn fixed(engine: Arc<AuthorizationEngine>, subject: Arc<Subject>) -> Arc<Self> {
        Self::sequence(vec![Some(CurrentAccess { engine, subject })])
    }

    /// One answer per read (`None`: the account is no longer active), the
    /// last repeated. An empty sequence reads as an inactive account.
    pub fn sequence(steps: Vec<Option<CurrentAccess>>) -> Arc<Self> {
        let answers = steps
            .into_iter()
            .map(|step| step.map_or(Answer::Inactive, Answer::Granted))
            .collect();
        Arc::new(Self {
            answers: Mutex::new(answers),
        })
    }

    /// Every read fails, as when the database cannot be reached.
    pub fn failing() -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(vec![Answer::Fails]),
        })
    }
}

#[async_trait]
impl AccessSource for TestAccess {
    async fn current(&self) -> anyhow::Result<StepAccess> {
        let mut answers = self.answers.lock().expect("access answers");
        let answer = if answers.len() > 1 {
            &answers.remove(0)
        } else {
            match answers.first() {
                Some(answer) => answer,
                None => &Answer::Inactive,
            }
        };
        match answer {
            Answer::Granted(access) => Ok(StepAccess::Granted(access.clone())),
            Answer::Inactive => Ok(StepAccess::Ended(ACCOUNT_INACTIVE.into())),
            Answer::Fails => anyhow::bail!("access unreadable"),
        }
    }
}
