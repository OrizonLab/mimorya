//! Outgoing e-mail (invitations, ADR-027; password reset links, ADR-028):
//! one plain-text message at a time, over SMTP when the operator configures
//! it (`SMTP_URL`, `MAIL_FROM`). Without it, nothing is sent: the
//! administrator transmits invitation links, and no reset link can be asked.

mod expiry;
mod settings;
mod smtp;

use async_trait::async_trait;

pub use expiry::expiry_label;
pub use settings::MailSettings;
pub use smtp::SmtpMailer;

/// One message to one recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub to_name: String,
    pub subject: String,
    pub text: String,
}

#[async_trait]
pub trait Mailer: Send + Sync {
    /// Sends the message; the error says why in French, without credentials.
    async fn send(&self, mail: &Mail) -> anyhow::Result<()>;
}
