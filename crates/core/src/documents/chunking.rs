//! A document's text cut into passages (ADR-095), the pieces the agent
//! finds and cites: contiguous, never across a page, cut at paragraphs,
//! then at sentences, then between words, each under the bound the store
//! keeps. A Markdown title (`#`) starts a passage, and names the passages
//! after it until the next.

use super::Extracted;

/// The length a passage is filled up to, in characters.
pub const PASSAGE_TARGET_CHARS: usize = 1_200;
/// The longest a passage may be, in characters.
pub const PASSAGE_MAX_CHARS: usize = 2_000;
/// The longest a passage's title is kept.
const HEADING_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    /// Its page, from 1, when the document has pages.
    pub page: Option<u32>,
    /// The title of the section it is in.
    pub heading: Option<String>,
    pub content: String,
}

/// The passages of `extracted`, in order.
pub fn passages(extracted: &Extracted) -> Vec<Passage> {
    let mut cut = Cutter::default();
    for (index, text) in extracted.pages.iter().enumerate() {
        cut.page = extracted
            .paged
            .then(|| u32::try_from(index + 1).unwrap_or(u32::MAX));
        for block in text.split("\n\n").flat_map(at_titles) {
            if let Some(heading) = heading_of(block) {
                cut.flush();
                cut.heading = Some(heading);
            }
            for piece in pieces(block) {
                cut.add(piece);
            }
        }
        cut.flush();
    }
    cut.done
}

#[derive(Default)]
struct Cutter {
    page: Option<u32>,
    heading: Option<String>,
    current: String,
    done: Vec<Passage>,
}

impl Cutter {
    fn add(&mut self, piece: String) {
        let joined = chars(&self.current) + 2 + chars(&piece);
        if !self.current.is_empty() && joined > PASSAGE_TARGET_CHARS {
            self.flush();
        }
        if !self.current.is_empty() {
            self.current.push_str("\n\n");
        }
        self.current.push_str(&piece);
    }

    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.done.push(Passage {
            page: self.page,
            heading: self.heading.clone(),
            content: std::mem::take(&mut self.current),
        });
    }
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// `block` cut before each of its lines that is a title: Markdown needs
/// no blank line before one.
fn at_titles(block: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    for line in block.split_inclusive('\n') {
        if at > start && heading_of(line).is_some() {
            parts.push(&block[start..at]);
            start = at;
        }
        at += line.len();
    }
    parts.push(&block[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// The title a block starts with, if it starts with a Markdown one.
fn heading_of(block: &str) -> Option<String> {
    let line = block.lines().next()?;
    let hashes = line.chars().take_while(|c| *c == '#').count();
    let title = line[hashes..].strip_prefix(' ')?.trim();
    ((1..=6).contains(&hashes) && !title.is_empty())
        .then(|| title.chars().take(HEADING_MAX_CHARS).collect())
}

/// `block` in pieces of at most `PASSAGE_MAX_CHARS`: whole when it fits,
/// else its sentences, grouped, a sentence too long cut between words.
fn pieces(block: &str) -> Vec<String> {
    if chars(block) <= PASSAGE_MAX_CHARS {
        return vec![block.to_string()];
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for part in sentences(block).into_iter().flat_map(between_words) {
        if !current.is_empty() && chars(&current) + 1 + chars(&part) > PASSAGE_MAX_CHARS {
            pieces.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&part);
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

/// The sentences of `text`: each ends after `.`, `!`, `?` or `…` followed
/// by a space or a line break.
fn sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut previous = None;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() && matches!(previous, Some('.' | '!' | '?' | '…')) {
            let sentence = text[start..at].trim();
            if !sentence.is_empty() {
                sentences.push(sentence);
            }
            start = at;
        }
        previous = Some(c);
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        sentences.push(rest);
    }
    sentences
}

/// `sentence` cut between words, each piece at most `PASSAGE_MAX_CHARS`;
/// a word longer than that, cut within.
fn between_words(sentence: &str) -> Vec<String> {
    if chars(sentence) <= PASSAGE_MAX_CHARS {
        return vec![sentence.to_string()];
    }
    let mut pieces = Vec::new();
    let mut current = String::new();
    for word in sentence.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while word.len() > PASSAGE_MAX_CHARS {
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
            }
            pieces.push(word.drain(..PASSAGE_MAX_CHARS).collect());
        }
        let word: String = word.into_iter().collect();
        if !current.is_empty() && chars(&current) + 1 + chars(&word) > PASSAGE_MAX_CHARS {
            pieces.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(pages: &[&str], paged: bool) -> Extracted {
        Extracted {
            pages: pages.iter().map(|page| page.to_string()).collect(),
            paged,
        }
    }

    #[test]
    fn short_paragraphs_fill_one_passage_never_across_a_page() {
        let cut = passages(&read(&["Un.\n\nDeux.", "Trois."], true));
        assert_eq!(
            cut,
            [
                Passage {
                    page: Some(1),
                    heading: None,
                    content: "Un.\n\nDeux.".into()
                },
                Passage {
                    page: Some(2),
                    heading: None,
                    content: "Trois.".into()
                },
            ]
        );
        let unpaged = passages(&read(&["Texte."], false));
        assert_eq!(unpaged[0].page, None);
    }

    #[test]
    fn a_title_starts_a_passage_and_names_those_after_it() {
        let text = "Préambule.\n\n# Chaudières\n\nEntretien annuel.\n\n## Fioul\n\nPurge.";
        let cut = passages(&read(&[text, "Suite fioul."], true));
        let named: Vec<_> = cut
            .iter()
            .map(|p| (p.page, p.heading.as_deref(), p.content.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                (Some(1), None, "Préambule."),
                (
                    Some(1),
                    Some("Chaudières"),
                    "# Chaudières\n\nEntretien annuel."
                ),
                (Some(1), Some("Fioul"), "## Fioul\n\nPurge."),
                (Some(2), Some("Fioul"), "Suite fioul."),
            ]
        );
        // Titles without a blank line before them.
        let tight = "# Procédure\nCouper.\n## Étape 1\nPurger.\n## Étape 2\nRelever.";
        let named: Vec<_> = passages(&read(&[tight], false))
            .into_iter()
            .map(|p| (p.heading.unwrap_or_default(), p.content))
            .collect();
        assert_eq!(
            named,
            [
                ("Procédure".to_string(), "# Procédure\nCouper.".to_string()),
                ("Étape 1".to_string(), "## Étape 1\nPurger.".to_string()),
                ("Étape 2".to_string(), "## Étape 2\nRelever.".to_string()),
            ]
        );
        // Not a title: no space after, or seven marks.
        assert_eq!(passages(&read(&["#hashtag"], false))[0].heading, None);
        assert_eq!(passages(&read(&["####### sept"], false))[0].heading, None);
    }

    #[test]
    fn paragraphs_fill_passages_up_to_their_target() {
        let paragraph = "a".repeat(500);
        let text = [paragraph.as_str(); 5].join("\n\n");
        let cut = passages(&read(&[&text], false));
        let lengths: Vec<_> = cut.iter().map(|p| chars(&p.content)).collect();
        assert_eq!(lengths, [1002, 1002, 500]);
    }

    #[test]
    fn a_long_paragraph_is_cut_at_its_sentences_then_between_words() {
        let sentence = format!("{}.", "mot ".repeat(150).trim_end());
        let paragraph = [sentence.as_str(); 8].join(" ");
        let cut = passages(&read(&[&paragraph], false));
        assert!(cut.len() > 1);
        for passage in &cut {
            assert!(chars(&passage.content) <= PASSAGE_MAX_CHARS);
            assert!(passage.content.ends_with('.'), "{}", passage.content);
        }
        let word = "x".repeat(PASSAGE_MAX_CHARS * 2 + 10);
        let cut = passages(&read(&[&word], false));
        let lengths: Vec<_> = cut.iter().map(|p| chars(&p.content)).collect();
        assert_eq!(lengths, [PASSAGE_MAX_CHARS, PASSAGE_MAX_CHARS, 10]);
        let joined: String = cut.iter().map(|p| p.content.as_str()).collect();
        assert_eq!(joined, word);
    }

    #[test]
    fn nothing_is_lost_but_blank_lines() {
        let text = "Ligne une.\nLigne deux.\n\n\n\nAutre paragraphe ! Et la suite ?";
        let cut = passages(&read(&[text], false));
        assert_eq!(
            cut[0].content,
            "Ligne une.\nLigne deux.\n\nAutre paragraphe ! Et la suite ?"
        );
        assert!(passages(&read(&["", "  "], true)).is_empty());
    }
}
