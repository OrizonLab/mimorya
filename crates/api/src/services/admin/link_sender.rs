//! What an administrator's one-time links need besides the database: the
//! mailer, if any, and where links lead (invitations, ADR-027; reset links,
//! ADR-029).

use mimorya_db::Database;

use crate::mail::Mailer;
use crate::services::AccessService;

pub struct LinkSender<'a> {
    pub db: &'a Database,
    /// Whether an account keeps its password under a required provider.
    pub accounts: &'a AccessService,
    pub mailer: Option<&'a dyn Mailer>,
    /// The web application's address, where the link leads.
    pub web_origin: &'a str,
    pub product: &'a str,
}
