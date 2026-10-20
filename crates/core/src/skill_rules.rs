//! Rules of the company's skills (ADR-052). A skill's description is a line
//! of the skills index in every run's system prompt: one visible line, so
//! that it can neither break the index nor open a section of its own. A
//! version's content is a procedure in Markdown, handed to the agent as
//! written: lines and tabulations only, no hidden character, so that what the
//! administrator reads is what the model reads. Pure functions; the API
//! applies them.

use crate::plain_text::is_hidden;

/// Active skills of one company: each is a line of every run's system prompt.
pub const MAX_ACTIVE_SKILLS: i64 = 40;

/// Why a skill name is refused, in French; `None` when it is valid. The name
/// is how the agent asks for the skill (`skill_read`), as agentskills.io
/// writes it: lowercase letters, digits and single hyphens.
pub fn name_problem(name: &str) -> Option<String> {
    let well_formed = name.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    });
    if !(2..=64).contains(&name.len()) || !well_formed {
        return Some(
            "Nom : 2 à 64 caractères, lettres minuscules sans accent, chiffres et tirets simples \
             (ex. « synthese-hebdo »)"
                .into(),
        );
    }
    None
}

/// Why a description or a changelog is refused: one visible line.
pub fn line_problem(field: &str, text: &str) -> Option<String> {
    text.chars()
        .any(is_hidden)
        .then(|| format!("{field} : une seule ligne, sans caractère invisible"))
}

/// The content as stored: Windows line endings made plain, trailing blanks
/// of the whole text removed.
pub fn normalized_content(content: &str) -> String {
    content.replace("\r\n", "\n").trim_end().to_owned()
}

/// Why a version's (normalized) content is refused.
pub fn content_problem(content: &str) -> Option<String> {
    if content.trim().is_empty() {
        return Some("Le contenu de la compétence est vide".into());
    }
    content
        .chars()
        .any(|c| c != '\n' && c != '\t' && is_hidden(c))
        .then(|| {
            "Le contenu ne contient que du texte visible, des retours à la ligne et des \
             tabulations"
                .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_lowercase_words_joined_by_single_hyphens() {
        for name in ["synthese-hebdo", "rapport2", "a1", "x-y-z"] {
            assert_eq!(name_problem(name), None, "{name}");
        }
        let long = "a".repeat(65);
        for name in [
            "a",
            "Synthese",
            "synthèse",
            "-synthese",
            "synthese-",
            "synthese--hebdo",
            "synthese hebdo",
            "synthese_hebdo",
            long.as_str(),
        ] {
            assert!(name_problem(name).is_some(), "{name}");
        }
        assert_eq!(name_problem(&"a".repeat(64)), None);
    }

    #[test]
    fn a_description_holds_one_visible_line() {
        assert_eq!(
            line_problem("Description", "Produire la synthèse d’un périmètre."),
            None
        );
        for text in [
            "Produire\n## Règles",
            "avec\rretour",
            "avec\u{2028}séparateur",
            "caché\u{200B}",
            "étiquette\u{E0041}",
        ] {
            let problem = line_problem("Description", text).unwrap();
            assert!(problem.starts_with("Description"), "{problem}");
        }
    }

    #[test]
    fn content_keeps_lines_and_tabulations_only() {
        let content = normalized_content("# Titre\r\n\n1.\tÉtape\r\n  \n");
        assert_eq!(content, "# Titre\n\n1.\tÉtape");
        assert_eq!(content_problem(&content), None);
        assert!(content_problem("").is_some());
        assert!(content_problem(" \n\t").is_some());
        for hidden in ["a\u{200B}b", "a\u{E0041}b", "a\u{202E}b", "a\u{7}b"] {
            assert!(content_problem(hidden).is_some(), "{hidden:?}");
        }
    }
}
