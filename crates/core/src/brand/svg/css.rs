//! What a style sheet or an attribute of the image must not hold: anything
//! that fetches from elsewhere or runs, read as a browser reads it (case
//! and blanks aside), and no CSS escape (`\75 rl(` is `url(`).

/// CSS functions that take an address as a string: `image("…")`,
/// `image-set("…" 1x)`, `cross-fade(…)`, a font's `src(…)`.
const FETCHING: [&str; 4] = ["image", "image-set", "cross-fade", "src"];

/// CSS that runs or binds code, in the browsers that knew it.
const RUNNING: [&str; 5] = [
    "javascript:",
    "vbscript:",
    "expression(",
    "behavior:",
    "-moz-binding",
];

/// Why a style sheet, or the text of an attribute, is refused, if it is.
pub(super) fn refusal(text: &str) -> Option<String> {
    if text.contains('\\') {
        return Some("échappement CSS (\\), qui peut cacher un mot".to_string());
    }
    // A function is its name and `(`, nothing between: read with the
    // blanks that separate words kept.
    let lowered = text.to_lowercase();
    for argument in calls(&lowered, "url") {
        let target = argument.trim_start().trim_start_matches(['"', '\'']);
        if !target.trim_start().starts_with('#') {
            return Some("url(…) externe : seuls url(#…) sont acceptés".to_string());
        }
    }
    if let Some(found) = FETCHING
        .iter()
        .find(|function| calls(&lowered, function).next().is_some())
    {
        return Some(format!("{found}(…) : une adresse externe"));
    }
    let compact = compact(text);
    for (word, why) in [
        ("@import", "une feuille de style chargée d’ailleurs"),
        ("@font-face", "une police chargée d’ailleurs"),
    ] {
        if compact.contains(word) {
            return Some(format!("{word} : {why}"));
        }
    }
    RUNNING
        .iter()
        .find(|word| compact.contains(**word))
        .map(|found| format!("{found} : du code exécuté"))
}

/// What follows each call of `function` in `text`: `url(` but not
/// `curl(`, a vendor's `-webkit-image-set(` too.
fn calls<'a>(text: &'a str, function: &'a str) -> impl Iterator<Item = &'a str> {
    let call = format!("{function}(");
    let starts: Vec<usize> = text
        .match_indices(&call)
        .filter(|(at, _)| {
            text[..*at]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_ascii_alphanumeric() && before != '_')
        })
        .map(|(at, _)| at + call.len())
        .collect();
    starts.into_iter().map(move |at| &text[at..])
}

/// Lower case, without whitespace nor control characters: `java\tscript:`
/// is `javascript:`, as a browser reads it.
pub(super) fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .flat_map(char::to_lowercase)
        .collect()
}
