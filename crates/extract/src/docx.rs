//! A Word document (.docx): the text of its body (`word/document.xml`),
//! paragraph by paragraph, its titles marked as Markdown ones (`#`) for the
//! passages to be named after them. The archive is opened warily: a bounded
//! number of entries, that part read up to a bound and no more than
//! `MAX_COMPRESSION_RATIO` times its compressed size (a decompression bomb
//! stops there), an encrypted entry refused. The XML is read without
//! expanding any entity but those XML predefines.

use std::io::{Cursor, Read};

use mimorya_core::documents::limits::{
    MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_PART_BYTES, MAX_COMPRESSION_RATIO,
};
use mimorya_core::documents::{Extracted, Refusal};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use zip::ZipArchive;
use zip::result::ZipError;

use crate::clean;

const BODY: &str = "word/document.xml";

pub fn read_docx(bytes: &[u8]) -> Result<Extracted, Refusal> {
    let xml = body(bytes)?;
    let text = paragraphs(&xml)?;
    Extracted::checked(vec![clean(&text)], false)
}

/// The document's body, inflated within bounds.
fn body(bytes: &[u8]) -> Result<Vec<u8>, Refusal> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| Refusal::Unreadable)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(Refusal::Unreadable);
    }
    let entry = match archive.by_name(BODY) {
        Ok(entry) => entry,
        Err(ZipError::UnsupportedArchive(reason)) if reason == ZipError::PASSWORD_REQUIRED => {
            return Err(Refusal::Encrypted);
        }
        Err(_) => return Err(Refusal::Unreadable),
    };
    // The compressed size the archive declares, never beyond the file's.
    let compressed = entry.compressed_size().min(bytes.len() as u64);
    let bound = compressed
        .max(1)
        .saturating_mul(MAX_COMPRESSION_RATIO)
        .min(MAX_ARCHIVE_PART_BYTES);
    let mut xml = Vec::new();
    entry
        .take(bound + 1)
        .read_to_end(&mut xml)
        .map_err(|_| Refusal::Unreadable)?;
    if xml.len() as u64 > bound {
        return Err(Refusal::Unreadable);
    }
    Ok(xml)
}

/// The paragraphs of the body: text runs, tabs as spaces, breaks as line
/// breaks, a title's paragraph led by as many `#` as its level.
fn paragraphs(xml: &[u8]) -> Result<String, Refusal> {
    let mut reader = Reader::from_reader(xml);
    let mut text = String::new();
    let mut paragraph = String::new();
    let mut level = None;
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(|_| Refusal::Unreadable)? {
            // An empty text run (`<w:t/>`) holds nothing, and closes nothing.
            Event::Start(element) if element.local_name().as_ref() == "t" => in_text = true,
            Event::Start(element) | Event::Empty(element) => match element.local_name().as_ref() {
                "tab" => paragraph.push(' '),
                "br" | "cr" => paragraph.push('\n'),
                "pStyle" => level = heading_level(&element),
                _ => {}
            },
            Event::End(element) => match element.local_name().as_ref() {
                "t" => in_text = false,
                "p" => {
                    let line = std::mem::take(&mut paragraph);
                    if let Some(level) = level.take().filter(|_| !line.trim().is_empty()) {
                        text.push_str(&"#".repeat(level));
                        text.push(' ');
                    }
                    text.push_str(line.trim());
                    text.push_str("\n\n");
                }
                _ => {}
            },
            Event::Text(run) if in_text => paragraph.push_str(&run.xml10_content()),
            Event::GeneralRef(reference) if in_text => {
                if let Some(c) = predefined(&reference) {
                    paragraph.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(text)
}

/// The level of a paragraph style naming a title: « Heading2 », « Titre2 »
/// (the French Word's) or « Title ».
fn heading_level(style: &BytesStart<'_>) -> Option<usize> {
    let value = style
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == "val")?
        .value
        .to_lowercase();
    if value == "title" || value == "titre" {
        return Some(1);
    }
    let digits = value
        .strip_prefix("heading")
        .or_else(|| value.strip_prefix("titre"))?;
    digits
        .parse::<usize>()
        .ok()
        .filter(|level| (1..=6).contains(level))
}

/// A character reference, or one of the five entities XML predefines; any
/// other is left out, never expanded.
fn predefined(reference: &quick_xml::events::BytesRef<'_>) -> Option<char> {
    if reference.is_char_ref() {
        return reference.resolve_char_ref().ok().flatten();
    }
    match reference.xml10_content().as_ref() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}
