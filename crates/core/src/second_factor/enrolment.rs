//! What a person's authenticator app is given (ADR-200): the secret in
//! base32 (RFC 4648, without padding), typed by hand or read from the
//! `otpauth://` address its QR code holds (the « Key Uri Format » the apps
//! share), naming the platform and the account.

use super::totp::{DIGITS, STEP_SECONDS};

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
/// The name the app shows the code under.
pub const ISSUER: &str = "Mimorya";

/// `bytes` in base32, without padding.
pub fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    for chunk in bytes.chunks(5) {
        let mut block = [0u8; 5];
        block[..chunk.len()].copy_from_slice(chunk);
        let bits = block.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
        let symbols = (chunk.len() * 8).div_ceil(5);
        for i in 0..symbols {
            let index = (bits >> (35 - i * 5)) & 0x1f;
            out.push(char::from(ALPHABET[index as usize]));
        }
    }
    out
}

/// The bytes `text` holds in base32 (upper case, without padding), if it
/// is base32 whose last symbol carries no stray bits.
pub fn read_base32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let (mut bits, mut held) = (0u32, 0u32);
    for symbol in text.bytes() {
        let value = ALPHABET.iter().position(|a| *a == symbol)?;
        bits = (bits << 5) | value as u32;
        held += 5;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    // What is left over pads the last byte: whole symbols cannot be, nor
    // bits set.
    (held < 5 && bits == 0).then_some(out)
}

/// The address an authenticator app reads for `secret`, for the account
/// `email`.
pub fn otpauth_uri(secret: &[u8], email: &str) -> String {
    format!(
        "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
        issuer = encode(ISSUER),
        account = encode(email),
        secret = base32(secret),
    )
}

/// `text` percent-encoded but for RFC 3986's unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_vectors_of_rfc_4648() {
        for (bytes, text) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(base32(bytes.as_bytes()), text, "{bytes}");
        }
        // The RFC 6238 secret, as apps are given it.
        assert_eq!(
            base32(b"12345678901234567890"),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );
    }

    #[test]
    fn reads_back_what_it_writes() {
        for bytes in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            let text = base32(bytes.as_bytes());
            assert_eq!(read_base32(&text).as_deref(), Some(bytes.as_bytes()));
        }
        let secret = [0xa5; 20];
        assert_eq!(read_base32(&base32(&secret)).as_deref(), Some(&secret[..]));
        // Lower case, padding, a stray symbol or stray bits: none of it.
        for text in ["mzxw6", "MY======", "MZXW1", "MZ", "M"] {
            assert_eq!(read_base32(text), None, "{text}");
        }
    }

    #[test]
    fn names_the_platform_and_the_account() {
        let uri = otpauth_uri(b"12345678901234567890", "léa.f+x@neo.example");
        assert_eq!(
            uri,
            "otpauth://totp/Mimorya:l%C3%A9a.f%2Bx%40neo.example\
             ?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Mimorya\
             &algorithm=SHA1&digits=6&period=30"
        );
    }
}
