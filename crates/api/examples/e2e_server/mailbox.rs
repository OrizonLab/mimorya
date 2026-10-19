//! E-mails written as JSON files instead of sent, for the tests to read the
//! links they carry.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use mimorya_api::mail::{Mail, Mailer};
use serde_json::json;

pub struct DirectoryMailer {
    dir: PathBuf,
    sent: AtomicU64,
}

impl DirectoryMailer {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            sent: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl Mailer for DirectoryMailer {
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let number = self.sent.fetch_add(1, Ordering::SeqCst);
        let millis = chrono::Utc::now().timestamp_millis();
        let body = json!({
            "to": mail.to,
            "toName": mail.to_name,
            "subject": mail.subject,
            "text": mail.text,
        });
        // Written aside then renamed: a reader never sees half a message.
        let name = format!("{millis}-{number:04}.json");
        let partial = self.dir.join(format!(".{name}"));
        tokio::fs::write(&partial, body.to_string()).await?;
        tokio::fs::rename(&partial, self.dir.join(name)).await?;
        Ok(())
    }
}
