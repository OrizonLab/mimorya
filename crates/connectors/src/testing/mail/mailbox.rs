//! The folders of the simulated IMAP server (ADR-205): an inbox and, if
//! the test gives one, a folder of sent e-mails, each with its validity and
//! its e-mails (UID, whether read, arrival day, RFC 5322 text). It answers
//! what the mail tools ask: `LIST`, `EXAMINE`, `UID SEARCH` (`CHARSET`,
//! `TEXT`, `SINCE`, `ALL`), `FETCH` and `UID FETCH` (`UID`, `FLAGS`,
//! `RFC822.SIZE`, `BODY.PEEK[HEADER.FIELDS (…)]`, `BODY.PEEK[]<0.n>`).

use chrono::NaiveDate;
use mail_parser::MessageParser;

use super::imap::strings;

/// An e-mail of a folder.
#[derive(Debug, Clone)]
pub struct TestMessage {
    pub uid: u32,
    pub seen: bool,
    pub day: NaiveDate,
    pub raw: String,
}

/// A folder: its name as listed, whether it is marked `\Sent`, its
/// validity, its e-mails in the order of their UIDs.
#[derive(Debug, Clone)]
pub struct TestFolder {
    pub name: String,
    pub sent: bool,
    pub validity: u32,
    pub messages: Vec<TestMessage>,
}

/// What the server does when a client fetches e-mails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FetchBehaviour {
    /// Sends what was asked.
    #[default]
    Answers,
    /// Sends, first, the first e-mail of the folder, asked or not.
    AddsTheFirst,
    /// Announces an e-mail of 400 MB (less than the IMAP library would
    /// make room for), its size written with leading zeros, then sends it
    /// without end.
    Endless,
}

/// The folders of the server's accounts (every account sees them), and
/// how it answers.
#[derive(Debug, Clone, Default)]
pub struct Mailboxes {
    pub folders: Vec<TestFolder>,
    /// Whether it takes literals without a pause (`LITERAL+`).
    pub literal_plus: bool,
    pub fetch: FetchBehaviour,
}

impl Mailboxes {
    /// The folder named `name` (`INBOX` in any case).
    pub fn folder(&self, name: &str) -> Option<&TestFolder> {
        self.folders.iter().find(|folder| {
            folder.name == name
                || (folder.name.eq_ignore_ascii_case("INBOX") && name.eq_ignore_ascii_case("INBOX"))
        })
    }

    /// The answer to `LIST "" "*"`.
    pub fn list(&self) -> String {
        self.folders
            .iter()
            .map(|folder| {
                let marks = if folder.sent {
                    "\\HasNoChildren \\Sent"
                } else {
                    "\\HasNoChildren"
                };
                format!("* LIST ({marks}) \"/\" \"{}\"\r\n", folder.name)
            })
            .collect()
    }
}

impl TestFolder {
    /// The untagged answers to `EXAMINE`.
    pub fn examined(&self) -> String {
        let next = self.messages.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
        format!(
            "* FLAGS (\\Seen \\Answered)\r\n* {} EXISTS\r\n* 0 RECENT\r\n\
             * OK [UIDVALIDITY {}] validité\r\n* OK [UIDNEXT {next}] suivant\r\n",
            self.messages.len(),
            self.validity
        )
    }

    /// The UIDs of the e-mails matching `criteria` (the words of
    /// `UID SEARCH`, strings unquoted).
    pub fn search(&self, criteria: &[String]) -> Vec<u32> {
        let mut words = Vec::new();
        let mut since = None;
        let mut parts = criteria.iter();
        while let Some(part) = parts.next() {
            match part.to_ascii_uppercase().as_str() {
                "CHARSET" => {
                    parts.next();
                }
                "TEXT" => words.extend(parts.next().map(|w| w.to_lowercase())),
                "SINCE" => {
                    since = parts
                        .next()
                        .and_then(|day| NaiveDate::parse_from_str(day, "%d-%b-%Y").ok());
                }
                _ => {}
            }
        }
        self.messages
            .iter()
            .filter(|message| since.is_none_or(|day| message.day >= day))
            .filter(|message| {
                let text = searchable(&message.raw);
                words.iter().all(|word| text.contains(word.as_str()))
            })
            .map(|message| message.uid)
            .collect()
    }

    /// The e-mails a set names: by UID (`3,5`) or by place (`2:4`).
    pub fn named(&self, set: &str, by_uid: bool) -> Vec<(u32, &TestMessage)> {
        let placed = self
            .messages
            .iter()
            .enumerate()
            .map(|(at, m)| (at as u32 + 1, m));
        let wanted = |seq: u32, uid: u32| {
            let n = if by_uid { uid } else { seq };
            set.split(',').any(|part| match part.split_once(':') {
                Some((from, to)) => {
                    let from = from.parse().unwrap_or(u32::MAX);
                    let to = to.parse().unwrap_or(u32::MAX);
                    (from..=to).contains(&n)
                }
                None => part.parse() == Ok(n),
            })
        };
        placed.filter(|(seq, m)| wanted(*seq, m.uid)).collect()
    }
}

impl Mailboxes {
    /// What the server answers a command on its folders, once signed in,
    /// `examined` the folder examined; `None` for another command.
    pub fn answer(
        &self,
        tag: &str,
        command: &str,
        arguments: &str,
        examined: &mut Option<String>,
    ) -> Option<String> {
        let done = |what: &str| format!("{tag} OK {what} terminé\r\n");
        let folder = examined.as_deref().and_then(|name| self.folder(name));
        let (command, arguments) = match command {
            "UID" => {
                let (sub, rest) = arguments.split_once(' ').unwrap_or((arguments, ""));
                (format!("UID {}", sub.to_ascii_uppercase()), rest)
            }
            other => (other.to_string(), arguments),
        };
        Some(match command.as_str() {
            "LIST" => self.list() + &done("LIST"),
            "EXAMINE" | "SELECT" => {
                let name = strings(arguments).into_iter().next().unwrap_or_default();
                match self.folder(&name) {
                    Some(found) => {
                        *examined = Some(found.name.clone());
                        found.examined() + &format!("{tag} OK [READ-ONLY] {command} terminé\r\n")
                    }
                    None => format!("{tag} NO dossier inconnu\r\n"),
                }
            }
            "UID SEARCH" => match folder {
                Some(folder) => {
                    let uids: Vec<String> = folder
                        .search(&strings(arguments))
                        .iter()
                        .map(u32::to_string)
                        .collect();
                    format!("* SEARCH {}\r\n", uids.join(" ")).replace("SEARCH \r", "SEARCH\r")
                        + &done("SEARCH")
                }
                None => format!("{tag} BAD aucun dossier\r\n"),
            },
            "FETCH" | "UID FETCH" => match folder {
                Some(folder) => {
                    let (set, items) = arguments.split_once(' ').unwrap_or((arguments, ""));
                    let mut said = String::new();
                    if self.fetch == FetchBehaviour::AddsTheFirst {
                        said.extend(folder.messages.first().map(|m| fetched(1, m, items)));
                    }
                    for (seq, message) in folder.named(set, command == "UID FETCH") {
                        said.push_str(&fetched(seq, message, items));
                    }
                    said + &done("FETCH")
                }
                None => format!("{tag} BAD aucun dossier\r\n"),
            },
            _ => return None,
        })
    }
}

/// The answer to a `FETCH` of `items` for the e-mail at `seq`.
pub fn fetched(seq: u32, message: &TestMessage, items: &str) -> String {
    let mut parts = vec![format!("UID {}", message.uid)];
    if items.contains("FLAGS") {
        parts.push(
            if message.seen {
                "FLAGS (\\Seen)"
            } else {
                "FLAGS ()"
            }
            .into(),
        );
    }
    if items.contains("RFC822.SIZE") {
        parts.push(format!("RFC822.SIZE {}", message.raw.len()));
    }
    if let Some(fields) = between(items, "HEADER.FIELDS (", ")") {
        let names: Vec<String> = fields.split(' ').map(str::to_ascii_lowercase).collect();
        let mut kept = header_fields(&message.raw, &names);
        // A part asked for (`<0.n>`): its first n bytes, said from 0.
        let part = between(items, ")]<0.", ">").and_then(|max| max.parse().ok());
        let origin = match part {
            Some(max) => {
                let mut end = kept.len().min(max);
                while !kept.is_char_boundary(end) {
                    end -= 1;
                }
                kept.truncate(end);
                "<0>"
            }
            None => "",
        };
        parts.push(format!(
            "BODY[HEADER.FIELDS ({fields})]{origin} {{{}}}\r\n{kept}",
            kept.len()
        ));
    }
    if let Some(max) = between(items, "BODY.PEEK[]<0.", ">") {
        let max = max.parse().unwrap_or(usize::MAX);
        let mut end = message.raw.len().min(max);
        while !message.raw.is_char_boundary(end) {
            end -= 1;
        }
        let body = &message.raw[..end];
        parts.push(format!("BODY[]<0> {{{}}}\r\n{body}", body.len()));
    }
    format!("* {seq} FETCH ({})\r\n", parts.join(" "))
}

/// The text a search looks in: the e-mail as written, its subject and its
/// text decoded, in lower case.
fn searchable(raw: &str) -> String {
    let parsed = MessageParser::default().parse(raw.as_bytes());
    let subject = parsed.as_ref().and_then(|m| m.subject().map(String::from));
    let text = parsed
        .as_ref()
        .and_then(|m| m.body_text(0).map(|t| t.into_owned()));
    [Some(raw.to_string()), subject, text]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(&text[from..to])
}

/// The header lines named, with their continuations, then the blank line.
fn header_fields(raw: &str, names: &[String]) -> String {
    let head = raw.split("\r\n\r\n").next().unwrap_or_default();
    let mut kept = String::new();
    let mut keeping = false;
    for line in head.split("\r\n") {
        if line.starts_with([' ', '\t']) {
            if keeping {
                kept.push_str(line);
                kept.push_str("\r\n");
            }
            continue;
        }
        let name = line
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        keeping = names.contains(&name);
        if keeping {
            kept.push_str(line);
            kept.push_str("\r\n");
        }
    }
    kept.push_str("\r\n");
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> TestFolder {
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let raw = |subject: &str| {
            format!("Subject: {subject}\r\nFrom: a@b.fr\r\n\r\nTexte {subject}\r\n")
        };
        TestFolder {
            name: "INBOX".into(),
            sent: false,
            validity: 7,
            messages: vec![
                TestMessage {
                    uid: 3,
                    seen: true,
                    day: day(2),
                    raw: raw("=?UTF-8?Q?R=C3=A9union?="),
                },
                TestMessage {
                    uid: 8,
                    seen: false,
                    day: day(9),
                    raw: raw("Facture"),
                },
            ],
        }
    }

    #[test]
    fn searches_decoded_words_and_days() {
        let folder = folder();
        let text = |w: &str| vec!["CHARSET".into(), "UTF-8".into(), "TEXT".into(), w.into()];
        assert_eq!(folder.search(&text("réunion")), [3]);
        assert_eq!(folder.search(&["SINCE".into(), "5-Nov-2026".into()]), [8]);
        assert_eq!(folder.search(&["ALL".into()]), [3, 8]);
        assert_eq!(folder.named("2:2", false)[0].1.uid, 8);
        assert_eq!(folder.named("3,8", true).len(), 2);
    }

    #[test]
    fn fetches_header_fields_and_a_part_of_the_body() {
        let folder = folder();
        let answer = fetched(
            2,
            &folder.messages[1],
            "(UID FLAGS BODY.PEEK[HEADER.FIELDS (SUBJECT)])",
        );
        assert_eq!(
            answer,
            "* 2 FETCH (UID 8 FLAGS () BODY[HEADER.FIELDS (SUBJECT)] {20}\r\nSubject: Facture\r\n\r\n)\r\n"
        );
        let part = fetched(2, &folder.messages[1], "(UID BODY.PEEK[]<0.7>)");
        assert!(part.contains("BODY[]<0> {7}\r\nSubject)"), "{part}");
    }
}
