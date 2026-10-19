//! Sends over SMTP (lettre, TLS by rustls with the webpki roots). Each
//! sending is bounded in time; a failure is reported in French, with the
//! relay's answer but never the URL (it may carry credentials).

use std::time::Duration;

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use super::{Mail, MailSettings, Mailer};

const SEND_TIMEOUT: Duration = Duration::from_secs(15);

pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    pub fn new(settings: &MailSettings) -> anyhow::Result<Self> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(&settings.smtp_url)
            .map_err(|_| anyhow::anyhow!("SMTP_URL : URL de relais SMTP invalide"))?
            .timeout(Some(SEND_TIMEOUT))
            .build();
        Ok(Self {
            transport,
            from: settings.from.clone(),
        })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let address = mail
            .to
            .parse()
            .map_err(|_| anyhow::anyhow!("Adresse du destinataire invalide"))?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(Mailbox::new(Some(mail.to_name.clone()), address))
            .subject(&mail.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(mail.text.clone())
            .map_err(|_| anyhow::anyhow!("Message impossible à composer"))?;
        self.transport.send(message).await.map_err(|error| {
            anyhow::anyhow!("Le serveur d’e-mail n’a pas accepté le message ({error})")
        })?;
        Ok(())
    }
}
