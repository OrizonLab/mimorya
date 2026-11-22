//! An SVG image the platform serves as its logo or favicon runs nothing and
//! fetches nothing: no script, no event handler, no element that embeds a
//! document, no element outside SVG's namespace (but the inert ones image
//! editors keep), no reference to elsewhere. It is refused rather than
//! cleaned: the operator fixes the file once, and the image shown is the
//! one they made (ADR-222). What is read is the parsed document, entities
//! resolved, in UTF-8 only, as it is served (`charset=utf-8`): neither a
//! character reference (`&#106;avascript:`), a CSS escape, an element slid
//! into a style sheet nor another encoding (a browser reading
//! `ISO-2022-JP` would see what a UTF-8 reader takes for a comment) hides
//! anything.

mod css;
#[cfg(test)]
mod tests;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, XmlVersion};

const SVG: &str = "http://www.w3.org/2000/svg";

/// Namespaces image editors keep their metadata in: no browser gives their
/// elements any behaviour.
const EDITORS: [&str; 5] = [
    "http://www.inkscape.org/namespaces/inkscape",
    "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "http://creativecommons.org/ns#",
    "http://purl.org/dc/elements/1.1/",
];

/// SVG elements that run, embed or fetch.
const REFUSED_ELEMENTS: [&str; 5] = ["script", "foreignobject", "handler", "listener", "iframe"];

/// Attributes that load something in some namespace: none belongs in an
/// image.
const LOADING: [&str; 6] = ["src", "srcset", "action", "formaction", "poster", "data"];

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

/// Why the SVG is refused, if it is: a sentence that follows « SVG
/// refusé : ».
pub(super) fn refusal(text: &str) -> Option<String> {
    if let Some(control) = text
        .chars()
        .find(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Some(format!(
            "caractère de contrôle U+{:04X}, qu’un navigateur pourrait lire autrement",
            u32::from(control)
        ));
    }
    let mut reader = NsReader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut depth = 0_usize;
    let mut root_seen = false;
    // The depth of the `<style>` open, if one is: all its text is CSS.
    let mut style: Option<usize> = None;
    loop {
        let (namespace, event) = match reader.read_resolved_event() {
            Ok(read) => read,
            Err(error) => return Some(format!("XML illisible ({error})")),
        };
        let opens = matches!(event, Event::Start(_));
        match event {
            Event::Decl(declaration) => {
                if let Some(encoding) = declaration.encoding() {
                    let encoding = encoding.map(|encoding| encoding.to_ascii_lowercase());
                    if !matches!(encoding.as_deref(), Ok("utf-8" | "utf8")) {
                        return Some("encodage déclaré autre qu’UTF-8".to_string());
                    }
                }
            }
            Event::Start(element) | Event::Empty(element) => {
                let name = local(&element);
                if style.is_some() {
                    return Some(format!("élément <{name}> dans une feuille de style"));
                }
                if let Some(why) = namespace_refusal(&namespace, &name) {
                    return Some(why);
                }
                if depth == 0 {
                    if root_seen || name != "svg" {
                        return Some("le document n’est pas une seule image <svg>".to_string());
                    }
                    root_seen = true;
                }
                if let Some(why) = element_refusal(&name, &element, &reader) {
                    return Some(why);
                }
                if opens {
                    depth += 1;
                    if name == "style" {
                        style = Some(depth);
                    }
                }
            }
            Event::End(_) => {
                if style == Some(depth) {
                    style = None;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Text(run) if style.is_some() => {
                if let Some(why) = css::refusal(&run.xml10_content()) {
                    return Some(why);
                }
            }
            Event::CData(run) if style.is_some() => {
                if let Some(why) = css::refusal(&run.xml10_content()) {
                    return Some(why);
                }
            }
            Event::GeneralRef(reference) => {
                let known = reference.is_char_ref()
                    || matches!(&*reference, "lt" | "gt" | "amp" | "apos" | "quot");
                if !known {
                    return Some("entité XML non prédéfinie".to_string());
                }
                if style.is_some() {
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

/// An element outside SVG's namespace: an XHTML `<img>` loads its `src`
/// wherever it stands; an editor's metadata does nothing.
fn namespace_refusal(namespace: &ResolveResult<'_>, name: &str) -> Option<String> {
    match namespace {
        ResolveResult::Bound(bound) if bound.0 == SVG || EDITORS.contains(&bound.0) => None,
        ResolveResult::Bound(bound) => Some(format!(
            "élément <{name}> de l’espace de noms {} : seuls ceux de SVG sont acceptés",
            bound.0
        )),
        _ => Some(format!(
            "élément <{name}> hors de l’espace de noms de SVG (xmlns=\"{SVG}\")"
        )),
    }
}

fn element_refusal(
    name: &str,
    element: &BytesStart<'_>,
    reader: &NsReader<&[u8]>,
) -> Option<String> {
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
        if LOADING.contains(&key.as_str()) {
            return Some(format!(
                "attribut {key} refusé dans <{name}> : il charge un contenu"
            ));
        }
        let compact = css::compact(&value);
        if key == "href" && !value.trim_start().starts_with('#') {
            if compact.starts_with("javascript:") || compact.starts_with("vbscript:") {
                return Some(format!(
                    "adresse javascript: dans l’attribut href de <{name}>"
                ));
            }
            if !EMBEDDED_IMAGES.iter().any(|kind| compact.starts_with(kind)) {
                return Some(format!(
                    "référence externe dans <{name}> : seuls #… et une image PNG, JPEG, GIF \
                     ou WebP intégrée (data:) sont acceptés"
                ));
            }
        }
        if animation && key == "attributename" && compact.ends_with("href") {
            return Some(format!("<{name}> refusé : il change une référence"));
        }
        // Every value as CSS reads it: presentation attributes (`fill`,
        // `mask`…) and `style` alike; not an editor's, which no browser
        // reads (a Windows path to an export).
        let (namespace, _) = reader.resolver().resolve_attribute(attribute.key);
        let editor = matches!(namespace, ResolveResult::Bound(bound) if EDITORS.contains(&bound.0));
        if !editor && let Some(why) = css::refusal(&value) {
            return Some(format!("attribut {key} de <{name}> : {why}"));
        }
    }
    None
}
