//! An e-mail handed to the company's SMTP server (ADR-205), on a connection
//! signed in as the run's person: its sender and each recipient given
//! (`MAIL FROM`, `RCPT TO`), then its text, each answer awaited. What is
//! known of it when the exchange fails: refused, or cut before its text was
//! handed, nothing left; handed and its answer never read, perhaps sent.

use std::sync::atomic::{AtomicBool, Ordering};

use lettre::Address;
use lettre::transport::smtp::Error;
use lettre::transport::smtp::client::AsyncSmtpConnection;
use lettre::transport::smtp::commands::{Data, Mail, Rcpt};

use crate::outbound::{Cause, ReadBudget, TOO_LONG};
use crate::standard_mail::smtp::failed;

/// What the server may send while the e-mail is handed: an answer to each
/// command.
const SUBMISSION_READ_LIMIT: usize = 64 * 1024;
/// What a server refusing the e-mail is told.
pub const MAIL_REFUSED: &str = "Le serveur SMTP refuse l’e-mail";
/// What a server refusing it for now is told.
pub const MAIL_REFUSED_FOR_NOW: &str =
    "Le serveur SMTP refuse l’e-mail pour l’instant : réessayez plus tard";

/// Whether the e-mail's text was handed to the server: past that, a failure
/// leaves it perhaps sent.
#[derive(Debug, Default)]
pub struct Handed(AtomicBool);

impl Handed {
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn set(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Why an e-mail is not known to be sent.
#[derive(Debug)]
pub enum Unsent {
    /// Nothing left: refused, or cut before its text was handed.
    Nothing(Cause),
    /// Its text handed, no answer read: it may have left.
    Unsure(Cause),
}

/// The e-mail `text`, from `from` to each of `to`, handed on `connection`.
pub async fn submit(
    connection: &mut AsyncSmtpConnection,
    budget: &ReadBudget,
    envelope: (&Address, &[Address]),
    text: &[u8],
    handed: &Handed,
) -> Result<(), Unsent> {
    let (from, to) = envelope;
    budget.grant(SUBMISSION_READ_LIMIT);
    let before = |error| Unsent::Nothing(cause(budget, error));
    let sender = Mail::new(Some(from.clone()), vec![]);
    connection.command(sender).await.map_err(before)?;
    for recipient in to {
        let recipient = Rcpt::new(recipient.clone(), vec![]);
        connection.command(recipient).await.map_err(before)?;
    }
    connection.command(Data).await.map_err(before)?;
    handed.set();
    let answered = connection.message(text).await;
    answered.map(|_| ()).map_err(|error| {
        // A refusal is the server's answer: it did not take the e-mail.
        match error.is_permanent() || error.is_transient() {
            true => Unsent::Nothing(cause(budget, error)),
            false => Unsent::Unsure(cause(budget, error)),
        }
    })
}

/// What failed, told by the server's answer or by what the exchange did.
fn cause(budget: &ReadBudget, error: Error) -> Cause {
    let cause = if error.is_permanent() {
        Cause {
            summary: MAIL_REFUSED,
            detail: error.to_string(),
        }
    } else if error.is_transient() {
        Cause {
            summary: MAIL_REFUSED_FOR_NOW,
            detail: error.to_string(),
        }
    } else {
        failed(error)
    };
    match budget.spent() {
        true => Cause {
            summary: TOO_LONG,
            ..cause
        },
        false => cause,
    }
}
