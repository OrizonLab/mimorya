//! What a process tells the others of its runs' events (ADR-083), as the
//! payload of a PostgreSQL notice: the event's number, the others reading it
//! from the run's journal; or a text delta, carried whole. A notice names
//! the process that sent it, which ignores its own, and the run, never its
//! company: the others read it in the company of whom follows it. A payload
//! stays under PostgreSQL's limit: a long delta is split.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::BusEvent;
use crate::contract::AssistantDelta;

/// PostgreSQL refuses a payload of 8000 bytes or more.
const MAX_PAYLOAD: usize = 7900;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    #[serde(rename = "o")]
    pub origin: Uuid,
    #[serde(rename = "r")]
    pub run_id: Uuid,
    #[serde(flatten)]
    pub kind: NoticeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k")]
pub enum NoticeKind {
    /// The run recorded its event `seq`.
    #[serde(rename = "e")]
    Event {
        #[serde(rename = "s")]
        seq: i32,
    },
    /// The model wrote `text`, after the run's event `after` if known.
    #[serde(rename = "d")]
    Delta {
        #[serde(rename = "a")]
        after: Option<i32>,
        #[serde(rename = "x")]
        text: String,
    },
}

impl Notice {
    /// The payload announcing the run's event `seq`.
    pub fn event(origin: Uuid, run_id: Uuid, seq: i32) -> String {
        let notice = Notice {
            origin,
            run_id,
            kind: NoticeKind::Event { seq },
        };
        notice.encode()
    }

    /// The payloads carrying a text delta, split as PostgreSQL requires.
    pub fn deltas(origin: Uuid, run_id: Uuid, after: Option<i32>, text: &str) -> Vec<String> {
        let notice = Notice {
            origin,
            run_id,
            kind: NoticeKind::Delta {
                after,
                text: text.into(),
            },
        };
        let payload = notice.encode();
        let chars = text.chars().count();
        if payload.len() <= MAX_PAYLOAD || chars < 2 {
            return vec![payload];
        }
        let middle = text
            .char_indices()
            .nth(chars / 2)
            .map_or(text.len(), |(at, _)| at);
        let mut payloads = Self::deltas(origin, run_id, after, &text[..middle]);
        payloads.extend(Self::deltas(origin, run_id, after, &text[middle..]));
        payloads
    }

    /// The notice a payload carries; `None` when it is not one.
    pub fn decode(payload: &str) -> Option<Self> {
        serde_json::from_str(payload).ok()
    }

    fn encode(&self) -> String {
        // Identifiers, numbers and a text always serialise.
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// A text delta as the run's subscribers receive it.
pub(super) fn delta_event(text: &str) -> BusEvent {
    BusEvent {
        seq: None,
        event_type: "assistant_delta".into(),
        // A text always serialises.
        data: serde_json::to_value(AssistantDelta { text: text.into() }).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_notice_reads_back() {
        let (origin, run) = (Uuid::new_v4(), Uuid::new_v4());
        let payload = Notice::event(origin, run, 7);
        let notice = Notice::decode(&payload).unwrap();
        assert_eq!((notice.origin, notice.run_id), (origin, run));
        assert_eq!(notice.kind, NoticeKind::Event { seq: 7 });
        assert_eq!(Notice::decode("{\"k\":\"e\"}"), None);
        assert_eq!(Notice::decode("pas une notice"), None);
    }

    #[test]
    fn a_long_delta_is_split_under_the_limit_and_whole_again() {
        let (origin, run) = (Uuid::new_v4(), Uuid::new_v4());
        // Escaped six bytes each, accents and quotes: well over the limit.
        let text = "é\"\u{1}".repeat(2000);
        let payloads = Notice::deltas(origin, run, Some(3), &text);
        assert!(payloads.len() > 1);
        let mut whole = String::new();
        for payload in &payloads {
            assert!(payload.len() <= MAX_PAYLOAD, "{}", payload.len());
            match Notice::decode(payload).unwrap().kind {
                NoticeKind::Delta { after, text } => {
                    assert_eq!(after, Some(3));
                    whole.push_str(&text);
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(whole, text);
        // A short one is sent as it is.
        assert_eq!(Notice::deltas(origin, run, None, "Bonjour").len(), 1);
    }
}
