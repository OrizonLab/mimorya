//! Files built for tests (feature `testing`): PDFs with a text per page,
//! one protected by a password, one whose content inflates beyond the
//! process's memory, and Word archives from their parts.

use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use lopdf::content::{Content, Operation};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    dictionary,
};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// A PDF with one page per text, in Helvetica: Latin-1 characters only.
pub fn pdf(pages: &[&str]) -> Vec<u8> {
    save(document(
        pages.iter().map(|text| page_content(text)).collect(),
    ))
}

/// A PDF whose opening needs `user_password` (empty: anyone opens it,
/// restricted by its owner's password).
pub fn protected_pdf(pages: &[&str], user_password: &str) -> Vec<u8> {
    let mut document = document(pages.iter().map(|text| page_content(text)).collect());
    // Encryption keys derive from the file's identifier.
    let id = Object::string_literal(b"0123456789abcdef".to_vec());
    document.trailer.set("ID", vec![id.clone(), id]);
    let version = EncryptionVersion::V2 {
        document: &document,
        owner_password: "propriétaire",
        user_password,
        key_length: 128,
        permissions: Permissions::default(),
    };
    let state = EncryptionState::try_from(version).expect("an encryption state");
    document.encrypt(&state).expect("an encrypted document");
    save(document)
}

/// A PDF whose single page's content inflates to `inflated` bytes.
pub fn inflating_pdf(inflated: u64) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    let block = vec![b' '; 1024 * 1024];
    let mut written = 0;
    while written < inflated {
        encoder.write_all(&block).expect("compressed in memory");
        written += block.len() as u64;
    }
    let compressed = encoder.finish().expect("compressed in memory");
    let mut dictionary = Dictionary::new();
    dictionary.set("Filter", "FlateDecode");
    save(document(vec![Stream::new(dictionary, compressed)]))
}

fn page_content(text: &str) -> Stream {
    let latin1: Vec<u8> = text
        .chars()
        .map(|c| u8::try_from(c).unwrap_or(b'?'))
        .collect();
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![50.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(latin1)]),
            Operation::new("ET", vec![]),
        ],
    };
    Stream::new(
        Dictionary::new(),
        content.encode().expect("encoded content"),
    )
}

fn document(contents: Vec<Stream>) -> Document {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id =
        document.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let mut kids: Vec<Object> = Vec::new();
    for content in contents {
        let content_id = document.add_object(content);
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        kids.push(page_id.into());
    }
    let count = i64::try_from(kids.len()).expect("a page count");
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document
}

fn save(mut document: Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("a saved document");
    bytes
}

/// A Word archive whose body holds `paragraphs`, WordprocessingML ones
/// (`<w:p>…</w:p>`).
pub fn docx(paragraphs: &str) -> Vec<u8> {
    archive(&[("word/document.xml", body(paragraphs).as_bytes())])
}

/// The body part of a Word archive holding `paragraphs`.
pub fn body(paragraphs: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{paragraphs}</w:body></w:document>"
    )
}

/// A Word archive whose body is marked encrypted, as a password protects
/// it (the flag of its local and central headers).
pub fn encrypted_docx() -> Vec<u8> {
    let content = body("<w:p><w:r><w:t>Secret</w:t></w:r></w:p>");
    let mut bytes = archive(&[("word/document.xml", content.as_bytes())]);
    for (signature, flags_at) in [(&b"PK\x03\x04"[..], 6), (&b"PK\x01\x02"[..], 8)] {
        let header = bytes
            .windows(4)
            .position(|window| window == signature)
            .expect("a header");
        bytes[header + flags_at] |= 1;
    }
    bytes
}

/// A ZIP archive of `entries`, deflated.
pub fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in entries {
        writer.start_file(*name, options).expect("an entry");
        writer.write_all(content).expect("its content");
    }
    writer.finish().expect("an archive").into_inner()
}
