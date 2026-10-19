//! E-mail in tests: a mailer that records what it sends (or fails on
//! demand), a minimal SMTP relay on a local port, and the token of an
//! invitation link.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mimorya_api::mail::{Mail, Mailer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[derive(Default)]
pub struct RecordingMailer {
    sent: Mutex<Vec<Mail>>,
    failing: Mutex<bool>,
}

impl RecordingMailer {
    pub fn sent(&self) -> Vec<Mail> {
        self.sent.lock().expect("sent").clone()
    }

    /// Refuses every message from now on, as an unreachable relay would.
    pub fn fail(&self) {
        *self.failing.lock().expect("failing") = true;
    }
}

#[async_trait]
impl Mailer for RecordingMailer {
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        if *self.failing.lock().expect("failing") {
            anyhow::bail!("Le serveur d’e-mail n’a pas accepté le message (relais injoignable)");
        }
        self.sent.lock().expect("sent").push(mail.clone());
        Ok(())
    }
}

/// The token of an invitation link (`…/invitation#<token>`).
pub fn token_of(link: &str) -> String {
    link.split_once("/invitation#")
        .map(|(_, token)| token.to_string())
        .unwrap_or_else(|| panic!("invitation link expected, got {link}"))
}

/// The invitation link quoted in an e-mail's text.
pub fn link_in(mail: &Mail) -> String {
    mail.text
        .lines()
        .find(|line| line.contains("/invitation#"))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("no link in {}", mail.text))
}

/// A plain SMTP relay on a local port that accepts every message; returns
/// its `smtp://` URL and the raw messages it received.
pub async fn serve_smtp() -> (String, Arc<Mutex<Vec<String>>>) {
    serve_smtp_with(false).await
}

/// The same relay, refusing every recipient (`550`) when asked.
pub async fn serve_smtp_with(refuse_recipients: bool) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
    let address = listener.local_addr().expect("local address");
    let received = Arc::new(Mutex::new(Vec::new()));
    let store = received.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let store = store.clone();
            tokio::spawn(async move {
                let (read, mut write) = socket.into_split();
                let mut lines = BufReader::new(read).lines();
                let _ = write.write_all(b"220 relais.test ESMTP\r\n").await;
                let mut data: Option<String> = None;
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(message) = data.as_mut() {
                        if line == "." {
                            store
                                .lock()
                                .expect("received")
                                .push(data.take().unwrap_or_default());
                            let _ = write.write_all(b"250 OK\r\n").await;
                        } else {
                            message.push_str(&line);
                            message.push('\n');
                        }
                        continue;
                    }
                    let command = line.to_ascii_uppercase();
                    // Refused recipients are named back, as real relays do.
                    let refusal;
                    let reply: &[u8] = if command.starts_with("EHLO") || command.starts_with("HELO")
                    {
                        b"250 relais.test\r\n"
                    } else if refuse_recipients && command.starts_with("RCPT") {
                        let address = line.split_once(':').map(|(_, a)| a.trim()).unwrap_or("");
                        refusal = format!("550 5.1.1 {address}: Recipient address rejected\r\n");
                        refusal.as_bytes()
                    } else if command.starts_with("DATA") {
                        data = Some(String::new());
                        b"354 Go ahead\r\n"
                    } else if command.starts_with("QUIT") {
                        let _ = write.write_all(b"221 Bye\r\n").await;
                        break;
                    } else {
                        b"250 OK\r\n"
                    };
                    let _ = write.write_all(reply).await;
                }
            });
        }
    });
    (format!("smtp://{address}"), received)
}
