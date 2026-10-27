//! Text of what Google gives as HTML (ADR-122): an e-mail's snippet, its
//! entities decoded, and the text of an e-mail or an event's description
//! written in HTML, its tags taken out, its lines and its table cells kept
//! apart; a plain text reads as itself. Not a renderer: what the model
//! reads, from an answer already bounded.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Comments, and blocks whose content is not text.
static HIDDEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<head\b.*?</head\s*>",
    )
    .expect("valid regex")
});
/// Where a cell of a table ends: its text kept apart from the next one's.
static CELLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<\s*/\s*t[dh]\s*>").expect("valid regex"));
/// Marks a cell's end until the lines are cut: a character of private use,
/// so that a `|` of the text itself is kept.
const CELL_END: char = '\u{E000}';
/// What a cell's end reads as, within a line.
const CELL_GAP: &str = " | ";
/// Where a line ends in what the HTML shows.
static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*(br|hr|/p|/div|/li|/tr|/table|/ul|/ol|/h[1-6]|/blockquote)\b[^>]*>")
        .expect("valid regex")
});
/// Tags only, those of Word's namespaces (`<o:p>`, a name after its
/// prefix) too: an address or a link written between angle brackets in a
/// plain text (`<tel:0123456789>`, `<urn:isbn:…>`), or a lone `<`, stays.
static TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)</?[A-Za-z][A-Za-z0-9-]*(?::[A-Za-z][A-Za-z0-9-]*)?(?:\s[^>]*)?/?>|<![^>]*>|<\?[^>]*>",
    )
    .expect("valid regex")
});
static ENTITIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});").expect("valid regex")
});

/// The text an HTML document shows, its lines trimmed, blank lines kept
/// one at a time; the cells of a row apart (`a | b`). A plain text reads
/// as itself.
pub fn html_to_text(html: &str) -> String {
    // The marker is the platform's: one the e-mail holds is not a cell's.
    let own = html.replace(CELL_END, "\u{FFFD}");
    let shown = HIDDEN.replace_all(&own, "");
    let cells = CELLS.replace_all(&shown, CELL_END.to_string());
    let broken = BREAKS.replace_all(&cells, "\n");
    let bare = TAGS.replace_all(&broken, "");
    let decoded = decode_entities(&bare);
    // The last cell's end closes no gap; an empty cell before the first
    // value keeps its place, the values under their column.
    let rows: Vec<String> = decoded
        .split('\n')
        .map(|line| {
            line.trim_end_matches(|c: char| c == CELL_END || c.is_whitespace())
                .replace(CELL_END, CELL_GAP)
        })
        .collect();
    tidy(&rows.join("\n"))
}

/// The text with its HTML entities decoded; one unknown is kept as is.
pub fn decode_entities(text: &str) -> String {
    ENTITIES
        .replace_all(text, |found: &Captures| {
            let name = &found[1];
            named(name)
                .or_else(|| numeric(name))
                .map_or_else(|| found[0].to_string(), String::from)
        })
        .into_owned()
}

/// The named entities a French e-mail writes most.
fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "agrave" => 'à',
        "acirc" => 'â',
        "ccedil" => 'ç',
        "icirc" => 'î',
        "iuml" => 'ï',
        "ocirc" => 'ô',
        "ugrave" => 'ù',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "Eacute" => 'É',
        "Egrave" => 'È',
        "Ecirc" => 'Ê',
        "Agrave" => 'À',
        "Ccedil" => 'Ç',
        "oelig" => 'œ',
        "rsquo" => '’',
        "lsquo" => '‘',
        "rdquo" => '”',
        "ldquo" => '“',
        "laquo" => '«',
        "raquo" => '»',
        "euro" => '€',
        "hellip" => '…',
        "ndash" => '–',
        "mdash" => '—',
        "deg" => '°',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "bull" => '•',
        "middot" => '·',
        "times" => '×',
        _ => return None,
    })
}

fn numeric(name: &str) -> Option<char> {
    let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => name.strip_prefix('#')?.parse().ok()?,
    };
    // Nor the character that marks a cell's end.
    char::from_u32(code).filter(|c| (!c.is_control() || c.is_whitespace()) && *c != CELL_END)
}

/// Lines trimmed, spaces within a line collapsed, at most one blank line
/// between two.
pub fn tidy(text: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    for line in text.replace("\r\n", "\n").split('\n') {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && kept.last().is_none_or(String::is_empty) {
            continue;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(String::is_empty) {
        kept.pop();
    }
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_are_decoded_and_unknown_ones_kept() {
        assert_eq!(
            decode_entities(
                "L&#39;offre &amp; la facture &lt;3&gt; &#xE9;t&eacute; &laquo;&euro;&raquo; &inconnu;"
            ),
            "L'offre & la facture <3> été «€» &inconnu;"
        );
        // A control character is not made of an entity.
        assert_eq!(decode_entities("a&#0;b&#27;c"), "a&#0;b&#27;c");
        // Decoded once: an entity of an entity stays one.
        assert_eq!(decode_entities("&amp;lt;"), "&lt;");
    }

    #[test]
    fn html_reads_as_its_text() {
        let html = "<!DOCTYPE html><html><head><title>T</title></head><body><style>p{}</style>\
                    <p class=MsoNormal>Bonjour&nbsp;Léa,<o:p></o:p></p>\
                    <p>Voici   la <b>facture</b> de <st1:place w:st='on'>Lyon</st1:place>\
                    <v:imagedata src='logo.png' o:title=''/>.<br/>Merci</p>\
                    <!-- caché --><script>alert(1)</script><div></div><hr><div>Fin</div></body></html>";
        assert_eq!(
            html_to_text(html),
            "Bonjour Léa,\nVoici la facture de Lyon.\nMerci\n\nFin"
        );
    }

    #[test]
    fn the_cells_of_a_row_stay_apart() {
        let invoice = "<table><tr><th>Article</th><th>Qté</th><th>Prix</th></tr>\
                       <tr><td>Filtre</td><td>12</td><td>300,00 €</td></tr>\
                       <tr><td>Montant HT</td><td></td><td>1 200,00 €</td></tr></table><p>Merci</p>";
        assert_eq!(
            html_to_text(invoice),
            "Article | Qté | Prix\nFiltre | 12 | 300,00 €\nMontant HT | | 1 200,00 €\n\nMerci"
        );
    }

    #[test]
    fn an_empty_cell_keeps_its_place() {
        // A corner left empty: each value stays under its column.
        let week = "<table><tr><th></th><th>Lundi</th><th>Mardi</th></tr>\
                    <tr><th>Léa</th><td>8h</td><td>9h</td></tr></table>";
        assert_eq!(html_to_text(week), "| Lundi | Mardi\nLéa | 8h | 9h");
        let block = "<table><tr><td><p>A</p></td><td></td><td>C</td></tr></table>";
        assert_eq!(html_to_text(block), "A\n| | C");
    }

    #[test]
    fn only_the_cells_set_their_separator() {
        // The marker, written in the e-mail or as an entity, is not a cell's.
        let marked =
            "<p>a\u{E000}b &#xE000; &#57344;</p><table><tr><td>c</td><td>d</td></tr></table>";
        assert_eq!(html_to_text(marked), "a\u{FFFD}b &#xE000; &#57344;\nc | d");
    }

    #[test]
    fn a_plain_text_keeps_what_is_between_angle_brackets() {
        let plain = "Rejoindre la réunion<https://teams.example/l/1> ou contacter Léa \
                     <lea@exemple.fr> <mailto:lea@exemple.fr> <tel:0123456789>\n\
                     <urn:isbn:9782070612758> <Nom: Léa>\nsi a < 5 et b > 3\nOrdre du jour |";
        assert_eq!(html_to_text(plain), plain);
    }

    #[test]
    fn blank_lines_are_kept_one_at_a_time() {
        assert_eq!(tidy("\n\n a \r\n\r\n\n  b  c \n\n"), "a\n\nb c");
    }
}
