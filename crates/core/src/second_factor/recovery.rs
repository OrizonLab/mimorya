//! Recovery codes (ADR-200): a person who lost their authenticator signs
//! in with one of them, each once. Ten, of 80 random bits each, written in
//! base32 as four groups of four characters; kept as their SHA-256 alone,
//! shown once, when made. A code is read whatever its case, spaces or
//! dashes: what the person copied, as they copied it.

use sha2::{Digest, Sha256};

use super::enrolment::base32;

/// How many codes an account is given at a time.
pub const RECOVERY_CODES: usize = 10;
/// The random bytes of a code: 80 bits.
pub const CODE_BYTES: usize = 10;
/// Its characters, dashes aside.
const CODE_CHARS: usize = 16;

/// A code made of `bytes`, as shown: `ABCD-EFGH-IJKL-MNOP`.
pub fn recovery_code(bytes: &[u8; CODE_BYTES]) -> String {
    let plain = base32(bytes);
    let groups: Vec<&str> = (0..CODE_CHARS)
        .step_by(4)
        .map(|at| &plain[at..at + 4])
        .collect();
    groups.join("-")
}

/// The code `input` is, written as kept (sixteen characters, upper case),
/// if it is one.
pub fn read_recovery_code(input: &str) -> Option<String> {
    let plain: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let is_base32 = |c: char| c.is_ascii_uppercase() || ('2'..='7').contains(&c);
    (plain.len() == CODE_CHARS && plain.chars().all(is_base32)).then_some(plain)
}

/// What is kept of a code read by `read_recovery_code`.
pub fn recovery_digest(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reads_back_however_it_was_copied() {
        let code = recovery_code(&[0x42; CODE_BYTES]);
        assert_eq!(code, "IJBE-EQSC-IJBE-EQSC");
        let kept = read_recovery_code(&code).unwrap();
        assert_eq!(kept, "IJBEEQSCIJBEEQSC");
        for copied in [
            " ijbe-eqsc ijbe-eqsc ",
            "IJBEEQSCIJBEEQSC",
            "ijbe eqsc ijbe eqsc",
        ] {
            assert_eq!(read_recovery_code(copied).as_deref(), Some(kept.as_str()));
        }
        assert_eq!(recovery_digest(&kept), recovery_digest("IJBEEQSCIJBEEQSC"));
        assert_ne!(recovery_digest(&kept), recovery_digest("IJBEEQSCIJBEEQSD"));
    }

    #[test]
    fn anything_else_is_no_code() {
        for input in [
            "",
            "123456",
            "IJBE-EQSC-IJBE-EQS",
            "IJBE-EQSC-IJBE-EQSC1",
            "IJBE-EQSC-IJBE-EQS1",
            "IJBE-EQSC-IJBE-EQS8",
            "ÉJBE-EQSC-IJBE-EQSC",
        ] {
            assert_eq!(read_recovery_code(input), None, "{input}");
        }
    }
}
