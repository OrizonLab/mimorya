//! A deposited file's name and title (ADR-095): its last segment (a browser
//! may send a path), in canonical form, without hidden characters, trimmed;
//! its title, the name without its extension.

use crate::plain_text::{canonical, without_hidden};

/// The longest file name kept.
pub const MAX_FILE_NAME_CHARS: usize = 255;
/// The longest title kept.
pub const MAX_TITLE_CHARS: usize = 200;

/// The name `raw` is kept as; `None` when nothing visible is left, or too
/// much.
pub fn file_name(raw: &str) -> Option<String> {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let name = without_hidden(&canonical(last)).trim().to_string();
    let length = name.chars().count();
    (length > 0 && length <= MAX_FILE_NAME_CHARS && name != "." && name != "..").then_some(name)
}

/// The title of the file `name`: without its extension, or the name itself.
pub fn title_of(name: &str) -> String {
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.trim().is_empty() => stem.trim(),
        _ => name,
    };
    stem.chars().take(MAX_TITLE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_keeps_its_last_segment_visible_and_canonical() {
        assert_eq!(file_name("Procédure.pdf").as_deref(), Some("Procédure.pdf"));
        assert_eq!(
            file_name("C:\\Users\\moi\\Rapport.docx").as_deref(),
            Some("Rapport.docx")
        );
        assert_eq!(file_name("../../etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(
            file_name("note\u{202E}fdp.txt").as_deref(),
            Some("notefdp.txt")
        );
        assert_eq!(
            file_name("Proce\u{301}dure.md").as_deref(),
            Some("Procédure.md")
        );
        for refused in ["", "  ", "dossier/", "..", "\u{200B}"] {
            assert_eq!(file_name(refused), None, "{refused:?}");
        }
        assert_eq!(file_name(&"a".repeat(MAX_FILE_NAME_CHARS + 1)), None);
    }

    #[test]
    fn a_title_is_the_name_without_its_extension() {
        assert_eq!(title_of("Procédure de purge.pdf"), "Procédure de purge");
        assert_eq!(title_of("archive.tar.gz"), "archive.tar");
        assert_eq!(title_of(".bashrc"), ".bashrc");
        assert_eq!(title_of("sans-extension"), "sans-extension");
        assert_eq!(
            title_of(&format!("{}.pdf", "é".repeat(300)))
                .chars()
                .count(),
            MAX_TITLE_CHARS
        );
    }
}
