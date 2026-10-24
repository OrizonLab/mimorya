//! A Markdown or plain text file: UTF-8, its byte order mark dropped; a NUL
//! byte says it is not text.

use mimorya_core::documents::{Extracted, Refusal};

use crate::clean;

const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

pub fn read_text(bytes: &[u8]) -> Result<Extracted, Refusal> {
    let bytes = bytes.strip_prefix(BYTE_ORDER_MARK).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| Refusal::NotUtf8)?;
    if text.contains('\0') {
        return Err(Refusal::Unreadable);
    }
    Extracted::checked(vec![clean(text)], false)
}
