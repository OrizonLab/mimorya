//! What a decision asks (ADR-213): typed questions on one state, as Jev
//! takes them — a yes or no (`noul`), one option among at most 255
//! (`choice`), a level on an ordered scale of at most 10 (`score`) — and the
//! state, each field declared by its usage and cut to its bound with an
//! explicit mark, the whole well under Jev's limits. Never the whole
//! conversation, a secret, a binary attachment nor a token: a usage sends
//! only the fields it declares.

/// Jev's bounds on a question.
pub const MAX_OPTIONS: usize = 255;
pub const MAX_LEVELS: usize = 10;
/// The whole state, in characters: well under Jev's 32,000 tokens a
/// question, and what the main model reads of untrusted data at once.
pub const MAX_STATE_CHARS: usize = 20_000;
/// The mark left where a field was cut.
pub const TRUNCATED: &str = "[…] tronqué";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionKind {
    /// What counts as a yes, and as a no, if said.
    Noul {
        yes: Option<String>,
        no: Option<String>,
    },
    /// Each option's name and when it applies.
    Choice { options: Vec<(String, String)> },
    /// The levels, from the lowest.
    Score { levels: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionQuestion {
    pub name: String,
    pub instructions: String,
    pub kind: QuestionKind,
}

impl DecisionQuestion {
    pub fn noul(name: &str, instructions: &str) -> Self {
        Self {
            name: name.into(),
            instructions: instructions.into(),
            kind: QuestionKind::Noul {
                yes: None,
                no: None,
            },
        }
    }

    pub fn choice(name: &str, instructions: &str, options: &[(&str, &str)]) -> Self {
        let options = options
            .iter()
            .map(|(name, when)| ((*name).to_string(), (*when).to_string()))
            .collect();
        Self {
            name: name.into(),
            instructions: instructions.into(),
            kind: QuestionKind::Choice { options },
        }
    }

    pub fn score(name: &str, instructions: &str, levels: &[&str]) -> Self {
        Self {
            name: name.into(),
            instructions: instructions.into(),
            kind: QuestionKind::Score {
                levels: levels.iter().map(|level| (*level).to_string()).collect(),
            },
        }
    }

    /// Why Jev would refuse it, if so.
    fn problem(&self) -> Option<&'static str> {
        if self.name.is_empty()
            || !self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Some("question name");
        }
        match &self.kind {
            QuestionKind::Noul { .. } => None,
            QuestionKind::Choice { options } => {
                let names: std::collections::BTreeSet<&str> =
                    options.iter().map(|(name, _)| name.as_str()).collect();
                let wrong = options.is_empty()
                    || options.len() > MAX_OPTIONS
                    || names.len() != options.len()
                    || names.contains("");
                wrong.then_some("choice options")
            }
            QuestionKind::Score { levels } => {
                (levels.is_empty() || levels.len() > MAX_LEVELS).then_some("score levels")
            }
        }
    }
}

/// What a decision is asked about: named fields of text, each cut to its
/// bound, the whole to `MAX_STATE_CHARS`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct DecisionState {
    fields: Vec<(String, String)>,
}

impl DecisionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `value` as `name`, cut to `max_chars` (and to what the state
    /// may still hold), the cut marked; left out once the state is full.
    pub fn field(mut self, name: &str, value: &str, max_chars: usize) -> Self {
        let room = MAX_STATE_CHARS.saturating_sub(self.chars());
        let bound = max_chars.min(room);
        let mark = TRUNCATED.chars().count();
        let text = if value.chars().count() <= bound {
            value.to_string()
        } else if bound > mark {
            let mut cut: String = value.chars().take(bound - mark - 1).collect();
            cut.push(' ');
            cut.push_str(TRUNCATED);
            cut
        } else if bound == mark {
            TRUNCATED.to_string()
        } else {
            // No room left even for the mark: the field is left out.
            return self;
        };
        self.fields.push((name.to_string(), text));
        self
    }

    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }

    /// Every character it holds.
    pub fn chars(&self) -> usize {
        self.fields
            .iter()
            .map(|(_, text)| text.chars().count())
            .sum()
    }

    /// As Jev and the main model read it: an object of named texts.
    pub fn to_json(&self) -> serde_json::Value {
        let fields = self
            .fields
            .iter()
            .map(|(name, text)| (name.clone(), serde_json::Value::String(text.clone())));
        serde_json::Value::Object(fields.collect())
    }
}

/// Its fields' names and sizes alone: what a state holds is never printed,
/// should a request reach a journal (ADR-213).
impl std::fmt::Debug for DecisionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut fields = f.debug_map();
        for (name, text) in &self.fields {
            fields.entry(name, &format_args!("{} caractères", text.chars().count()));
        }
        fields.finish()
    }
}

/// A decision: its usage, its state and its questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRequest {
    /// The usage it serves, as journaled (`DecisionUsage::as_str`).
    pub usage: &'static str,
    pub state: DecisionState,
    pub questions: Vec<DecisionQuestion>,
}

impl DecisionRequest {
    /// Why no engine may be asked it, if so: a fault of the usage's code.
    pub fn problem(&self) -> Option<&'static str> {
        if self.questions.is_empty() {
            return Some("no question");
        }
        let names: std::collections::BTreeSet<&str> =
            self.questions.iter().map(|q| q.name.as_str()).collect();
        if names.len() != self.questions.len() {
            return Some("question names repeated");
        }
        self.questions.iter().find_map(DecisionQuestion::problem)
    }

    pub fn question(&self, name: &str) -> Option<&DecisionQuestion> {
        self.questions.iter().find(|question| question.name == name)
    }
}
