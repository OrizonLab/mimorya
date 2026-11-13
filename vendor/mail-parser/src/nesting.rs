//! Patch of the Mimorya platform (ADR-205): how many e-mails a part may be
//! enclosed in. The parser reads an enclosed e-mail (`message/*`) as an
//! e-mail within the one enclosing it; an encoded one is decoded, read by
//! the parser calling itself, and turned into an owned tree that calls
//! itself once per e-mail it encloses. Thousands of them, in less than a
//! megabyte any sender may send, overflow the stack and end the process:
//! past `MAX_NESTED_MESSAGES`, an enclosed e-mail is kept as an attachment
//! and never read as an e-mail.

/// The e-mails a part may be enclosed in: real messages forward a handful.
pub const MAX_NESTED_MESSAGES: usize = 16;

/// The e-mails the parser holds on its `stack` while it reads what they
/// enclose.
pub(crate) fn held<S, M>(stack: &[(S, Option<M>)]) -> usize {
    stack.iter().filter(|(_, message)| message.is_some()).count()
}

/// Whether a part enclosed in `enclosing` e-mails, those the parser holds
/// and those it decoded and reads by calling itself, is too deep to be read
/// as an e-mail itself.
pub(crate) fn too_deep(enclosing: usize) -> bool {
    enclosing >= MAX_NESTED_MESSAGES
}
