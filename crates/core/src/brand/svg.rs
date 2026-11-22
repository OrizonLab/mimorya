//! An SVG image the platform serves as its logo or favicon runs nothing: no
//! script, no event handler, no element that embeds a document, nothing
//! fetched from elsewhere. It is refused rather than cleaned: the operator
//! fixes the file once, and the image shown is the one they made (ADR-222).
//! What is read is the parsed document, entities resolved, so that neither
//! a character reference (`&#106;avascript:`) nor a CSS escape hides
//! anything.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// Elements that run, embed or fetch: refused whatever their namespace.
const REFUSED_ELEMENTS: [&str; 13] = [
    "script",
    "foreignobject",
    "iframe",
    "object",
    "embed",
    "handler",
    "listener",
    "audio",
    "video",
    "canvas",
    "link",
    "meta",
    "base",
];

/// Animations, refused when they change a reference (`href`): one could
/// turn a link into `javascript:` once shown.
const ANIMATIONS: [&str; 4] = ["set", "animate", "animatemotion", "animatetransform"];

/// The images a `data:` reference may embed: rasters only, an SVG inside
/// an SVG being read by no one here.
const EMBEDDED_IMAGES: [&str; 4] = [
    "data:image/png;",
    "data:image/jpeg;",
    "data:image/gif;",
    "data:image/webp;",
];

/// Why the SVG is refused, if it is: a sentence that ends the message
/// « le logo est refusé : … ».
pub(super) fn refusal(text: &str) -> Option<String> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut depth = 0_usize;
    let mut root_seen = false;
    let mut in_style = false;
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(error) => return Some(format!("XML illisible ({error})")),
        };
        let opens = matches!(event, Event::Start(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let name = local(&element);
                if depth == 0 {
                    if root_seen || name != "svg" {
                        return Some("le document n’est pas une seule image <svg>".to_string());
                    }
                    root_seen = true;
                }
                if let Some(why) = element_refusal(&name, &element) {
                    return Some(why);
                }
                if opens {
                    depth += 1;
                    in_style = name == "style";
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                in_style = false;
            }
            Event::Text(run) if in_style => {
                if let Some(why) = style_refusal(&run.xml10_content()) {
                    return Some(why);
                }
            }
            Event::CData(run) if in_style => {
                if let Some(why) = style_refusal(&run.xml10_content()) {
                    return Some(why);
                }
            }
            Event::GeneralRef(reference) => {
                let known = reference.is_char_ref()
                    || matches!(&*reference, "lt" | "gt" | "amp" | "apos" | "quot");
                if !known {
                    return Some("entité XML non prédéfinie".to_string());
                }
                if in_style {
                    // A character written by reference in a style sheet:
                    // nothing a logo needs, and a way to hide a word.
                    return Some("référence de caractère dans une feuille de style".to_string());
                }
            }
            Event::DocType(_) => {
                return Some("déclaration DOCTYPE (entités) refusée".to_string());
            }
            Event::PI(_) => {
                return Some(
                    "instruction de traitement (<?xml-stylesheet …?>) refusée".to_string(),
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    (!root_seen).then(|| "aucune image <svg>".to_string())
}

fn local(element: &BytesStart<'_>) -> String {
    element.local_name().as_ref().to_ascii_lowercase()
}

fn element_refusal(name: &str, element: &BytesStart<'_>) -> Option<String> {
    if REFUSED_ELEMENTS.contains(&name) {
        return Some(format!(
            "élément <{name}> refusé : il exécute ou intègre un contenu"
        ));
    }
    let animation = ANIMATIONS.contains(&name);
    for attribute in element.attributes().with_checks(true) {
        let Ok(attribute) = attribute else {
            return Some(format!("attribut illisible dans <{name}>"));
        };
        let key = attribute.key.local_name().as_ref().to_ascii_lowercase();
        let Ok(value) = attribute.normalized_value(XmlVersion::Implicit1_0) else {
            return Some(format!(
                "valeur illisible de l’attribut {key} dans <{name}>"
            ));
        };
        if key.starts_with("on") {
            return Some(format!(
                "attribut {key} refusé dans <{name}> : il exécute un script"
            ));
        }
        let compact = compact(&value);
        if compact.contains("javascript:") || compact.contains("vbscript:") {
            return Some(format!(
                "adresse javascript: dans l’attribut {key} de <{name}>"
            ));
        }
        if let Some(why) = external_url(&compact) {
            return Some(why);
        }
        if key == "href" && !value.trim_start().starts_with('#') {
            let embedded = EMBEDDED_IMAGES.iter().any(|kind| compact.starts_with(kind));
            if !embedded {
                return Some(format!(
                    "référence externe dans <{name}> : seuls #… et une image PNG, JPEG, GIF \
                     ou WebP intégrée (data:) sont acceptés"
                ));
            }
        }
        if animation && key == "attributename" && compact.ends_with("href") {
            return Some(format!("<{name}> refusé : il change une référence"));
        }
        if key == "style"
            && let Some(why) = style_refusal(&value)
        {
            return Some(why);
        }
    }
    None
}

/// What a style sheet must not do: import, fetch from elsewhere, run.
fn style_refusal(style: &str) -> Option<String> {
    if style.contains('\\') {
        return Some("échappement CSS (\\) dans un style".to_string());
    }
    let compact = compact(style);
    if compact.contains("@import") {
        return Some("@import dans un style".to_string());
    }
    if let Some(why) = external_url(&compact) {
        return Some(why);
    }
    for refused in ["javascript:", "expression(", "behavior:", "-moz-binding"] {
        if compact.contains(refused) {
            return Some(format!("{refused} dans un style"));
        }
    }
    None
}

/// A `url(…)` that fetches from elsewhere, in a style or an attribute
/// (`fill="url(…)"`): only a fragment of the image itself is taken.
fn external_url(compact: &str) -> Option<String> {
    for (at, _) in compact.match_indices("url(") {
        let target = compact[at + 4..].trim_start_matches(['"', '\'']);
        if !target.starts_with('#') {
            return Some("url(…) externe : seuls url(#…) sont acceptés".to_string());
        }
    }
    None
}

/// Lower case, without whitespace nor control characters: `java\tscript:`
/// is `javascript:`, as a browser reads it.
fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGO: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- Made by hand -->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 32 32">
  <title>Néo &amp; Cie</title>
  <defs>
    <linearGradient id="g"><stop offset="0" stop-color="#1f5f8b"/></linearGradient>
    <style>.a { fill: url(#g); }</style>
  </defs>
  <g style="fill: url('#g')"><path class="a" d="M0 0h32v32H0z"/></g>
  <use xlink:href="#g"/>
  <image href="data:image/png;base64,iVBORw0KGgo="/>
  <animate attributeName="opacity" from="0" to="1" dur="1s"/>
</svg>"##;

    fn wrapped(inner: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{inner}</svg>"#)
    }

    #[test]
    fn a_still_logo_is_taken() {
        assert_eq!(refusal(LOGO), None);
    }

    #[test]
    fn whatever_runs_or_embeds_is_refused() {
        for (inner, why) in [
            ("<script>alert(1)</script>", "élément <script> refusé"),
            ("<SCRIPT>alert(1)</SCRIPT>", "élément <script> refusé"),
            (
                r#"<h:script xmlns:h="http://www.w3.org/1999/xhtml">alert(1)</h:script>"#,
                "élément <script> refusé",
            ),
            (
                "<foreignObject><p>x</p></foreignObject>",
                "élément <foreignobject> refusé",
            ),
            (r#"<rect onload="alert(1)"/>"#, "attribut onload refusé"),
            (r#"<rect OnClick="alert(1)"/>"#, "attribut onclick refusé"),
            (
                r#"<a href="javascript:alert(1)"><rect/></a>"#,
                "adresse javascript:",
            ),
            (
                r#"<a href="&#106;avascript:alert(1)"><rect/></a>"#,
                "adresse javascript:",
            ),
            (
                r#"<a href="java&#9;script:alert(1)"><rect/></a>"#,
                "adresse javascript:",
            ),
            (
                r#"<a href="https://example.com/"><rect/></a>"#,
                "référence externe",
            ),
            (r#"<use href="other.svg#x"/>"#, "référence externe"),
            (
                r#"<image href="data:image/svg+xml;base64,PHN2Zz4="/>"#,
                "référence externe",
            ),
            (
                r#"<set attributeName="href" to="javascript:alert(1)"/>"#,
                "il change une référence",
            ),
            (
                r#"<set attributeName="fill" to="javascript:alert(1)"/>"#,
                "adresse javascript:",
            ),
            (
                r#"<rect fill="url(https://example.com/p.svg#a)"/>"#,
                "url(…) externe",
            ),
            (
                r##"<animate attributeName="xlink:href" values="#a;#b"/>"##,
                "il change une référence",
            ),
            (
                "<style>@import url(https://example.com/a.css);</style>",
                "@import",
            ),
            (
                "<style>.a { fill: url(https://example.com/p) }</style>",
                "url(…) externe",
            ),
            (
                "<style>.a { fill: u\\72 l(https://example.com/p) }</style>",
                "échappement CSS",
            ),
            (
                "<style>.a { fill: &#117;rl(x) }</style>",
                "référence de caractère",
            ),
            (
                r#"<rect style="background: url(//example.com/p)"/>"#,
                "url(…) externe",
            ),
            ("<style><![CDATA[@import 'x.css';]]></style>", "@import"),
            ("<iframe/>", "élément <iframe> refusé"),
        ] {
            let refused = refusal(&wrapped(inner)).unwrap_or_default();
            assert!(refused.contains(why), "{inner}: {refused}");
        }
    }

    #[test]
    fn what_could_hide_or_fetch_is_refused() {
        let doctype = r#"<!DOCTYPE svg [<!ENTITY x "javascript:">]><svg/>"#;
        assert!(refusal(doctype).unwrap().starts_with("déclaration DOCTYPE"));
        let stylesheet = r#"<?xml-stylesheet href="https://example.com/a.css"?><svg/>"#;
        assert!(
            refusal(stylesheet)
                .unwrap()
                .starts_with("instruction de traitement")
        );
        assert!(
            refusal(&wrapped("&unknown;"))
                .unwrap()
                .starts_with("entité XML")
        );
    }

    #[test]
    fn only_one_svg_image_is_an_svg() {
        for text in ["", "<html/>", "<svg/><svg/>", "texte", "<svg><g></svg>"] {
            assert!(refusal(text).is_some(), "{text}");
        }
    }
}
