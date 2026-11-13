//! The tools of the standard mail (ADR-205), each acting as the person the
//! run serves with the account they linked: how they reach that person's
//! IMAP session (`person`), where they look (`folders`), what a search asks
//! (`criteria`), what they keep of the e-mails the server sends (`fetched`,
//! `message`); searching (`search`) and reading (`read`) an e-mail; sending
//! one (`send`), handed to the SMTP server (`submission`).

mod criteria;
mod fetched;
mod folders;
mod message;
mod person;
mod read;
mod search;
mod send;
mod submission;

pub use person::MailReach;
pub use read::MailRead;
pub use search::MailSearch;
pub use send::MailSend;
