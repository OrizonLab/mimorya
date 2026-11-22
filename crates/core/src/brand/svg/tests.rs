//! What an SVG of the brand may hold, and every way found to slip a script
//! or a reference to elsewhere into one.

use super::refusal;

const LOGO: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- Made by hand, kept by Inkscape -->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
     xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
     xmlns:cc="http://creativecommons.org/ns#"
     viewBox="0 0 32 32" inkscape:export-filename="C:\Logos\neo.png" aria-label="Mon image (logo)">
  <title>Néo &amp; Cie</title>
  <sodipodi:namedview inkscape:zoom="2"/>
  <metadata><rdf:RDF><cc:Work rdf:about=""/></rdf:RDF></metadata>
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

fn refused(text: &str) -> String {
    refusal(text).unwrap_or_default()
}

#[test]
fn a_still_logo_and_an_editor_s_metadata_are_taken() {
    assert_eq!(refusal(LOGO), None);
}

#[test]
fn whatever_runs_or_embeds_is_refused() {
    for (inner, why) in [
        ("<script>alert(1)</script>", "élément <script> refusé"),
        ("<SCRIPT>alert(1)</SCRIPT>", "élément <script> refusé"),
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
            r#"<set attributeName="href" to="javascript:alert(1)"/>"#,
            "il change une référence",
        ),
        (
            r#"<set attributeName="fill" to="javascript:alert(1)"/>"#,
            "du code exécuté",
        ),
        (
            r##"<animate attributeName="xlink:href" values="#a;#b"/>"##,
            "il change une référence",
        ),
        ("<iframe/>", "élément <iframe> refusé"),
    ] {
        let refused = refused(&wrapped(inner));
        assert!(refused.contains(why), "{inner}: {refused}");
    }
}

#[test]
fn whatever_fetches_from_elsewhere_is_refused() {
    for (inner, why) in [
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
            r#"<rect fill="url(https://example.com/p.svg#a)"/>"#,
            "url(…) externe",
        ),
        (
            "<style>@import url(https://example.com/a.css);</style>",
            "url(…) externe",
        ),
        (
            "<style>@import 'https://example.com/a.css';</style>",
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
        (
            "<style>@font-face { font-family: a; src: local(a) }</style>",
            "@font-face",
        ),
        // An element slid into a style sheet hid what followed it.
        (
            "<style><g></g>@import url(https://evil.example/a.css);</style>",
            "élément <g> dans une feuille de style",
        ),
        // An element of another namespace loads what it names.
        (
            r#"<h:img xmlns:h="http://www.w3.org/1999/xhtml" src="https://evil.example/p.png"/>"#,
            "espace de noms http://www.w3.org/1999/xhtml",
        ),
        (
            r#"<image src="https://evil.example/p.png"/>"#,
            "attribut src refusé",
        ),
        // A CSS escape in a presentation attribute.
        (
            r#"<rect fill="\75 rl(https://evil.example/p.svg#a)"/>"#,
            "échappement CSS",
        ),
        // An address written as a string.
        (
            r#"<g style="background:image-set('https://evil.example/p.png' 1x)"/>"#,
            "image-set(…)",
        ),
        (
            r#"<g style="mask:-webkit-image-set('https://evil.example/p.png' 1x)"/>"#,
            "image-set(…)",
        ),
        (
            r#"<g style="mask:image('https://evil.example/p.png')"/>"#,
            "image(…)",
        ),
    ] {
        let refused = refused(&wrapped(inner));
        assert!(refused.contains(why), "{inner}: {refused}");
    }
}

#[test]
fn another_encoding_cannot_hide_a_script() {
    // Read in ISO-2022-JP, `ESC ( B` is nothing: the comment closes and a
    // script follows; read in UTF-8, it is all a comment.
    let hidden = "<?xml version=\"1.0\" encoding=\"ISO-2022-JP\"?>\
        <svg xmlns=\"http://www.w3.org/2000/svg\"><!-- -\u{1b}(B-><script>alert(1)</script> --></svg>";
    assert!(
        refused(hidden).starts_with("caractère de contrôle U+001B"),
        "{}",
        refused(hidden)
    );
    let latin = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    assert_eq!(refused(latin), "encodage déclaré autre qu’UTF-8");
    let utf8 =
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    assert_eq!(refusal(utf8), None);
}

#[test]
fn what_could_hide_is_refused() {
    let doctype = r#"<!DOCTYPE svg [<!ENTITY x "javascript:">]><svg/>"#;
    assert!(refused(doctype).starts_with("déclaration DOCTYPE"));
    let stylesheet = r#"<?xml-stylesheet href="https://example.com/a.css"?><svg/>"#;
    assert!(refused(stylesheet).starts_with("instruction de traitement"));
    assert!(refused(&wrapped("&unknown;")).starts_with("entité XML"));
}

#[test]
fn only_one_svg_image_is_an_svg() {
    for text in [
        "",
        "<html/>",
        "<svg/>",
        "texte",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><g></svg>"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg"/><svg xmlns="http://www.w3.org/2000/svg"/>"#,
    ] {
        assert!(refusal(text).is_some(), "{text}");
    }
    assert!(refused("<svg/>").contains("hors de l’espace de noms de SVG"));
}
