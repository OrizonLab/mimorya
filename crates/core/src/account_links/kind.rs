//! Which provider a link is at (ADR-121), and what follows from it: the
//! permissions a person is asked, those the tools need, those renewed, the
//! sign-in's extra parameters, and the account a token must prove. A
//! permission is compared on its last segment, case aside (Microsoft may
//! name a Graph permission by its address, Google always does).

use super::google::{self, CALENDAR_EVENTS, CALENDAR_READONLY, GMAIL_READONLY, GMAIL_SEND};
use super::microsoft365::{
    self, CALENDARS_READ, CALENDARS_WRITE, MAIL_READ, MAIL_SEND, OFFLINE_ACCESS,
};
use super::{GOOGLE_WORKSPACE, LinkRefusal, LinkedIdentity, MICROSOFT365};
use crate::identity_providers::TokenClaims;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Microsoft365,
    GoogleWorkspace,
}

impl LinkKind {
    pub const ALL: [Self; 2] = [Self::Microsoft365, Self::GoogleWorkspace];

    /// The kind of a connector type, if people link accounts there.
    pub fn of(connector_type: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.connector_type() == connector_type)
    }

    pub fn connector_type(self) -> &'static str {
        match self {
            Self::Microsoft365 => MICROSOFT365,
            Self::GoogleWorkspace => GOOGLE_WORKSPACE,
        }
    }

    /// The provider, as people read it.
    pub fn provider(self) -> &'static str {
        match self {
            Self::Microsoft365 => "Microsoft",
            Self::GoogleWorkspace => "Google",
        }
    }

    /// The product, as people read it.
    pub fn product(self) -> &'static str {
        match self {
            Self::Microsoft365 => "Microsoft 365",
            Self::GoogleWorkspace => "Google Workspace",
        }
    }

    /// What every link asks besides `openid`: the person's address, and for
    /// Microsoft the refresh token.
    fn identity(self) -> &'static [&'static str] {
        match self {
            Self::Microsoft365 => &["email", "profile", OFFLINE_ACCESS],
            Self::GoogleWorkspace => &["email", "profile"],
        }
    }

    fn reading(self) -> &'static [&'static str] {
        match self {
            Self::Microsoft365 => &[MAIL_READ, CALENDARS_READ],
            Self::GoogleWorkspace => &[GMAIL_READONLY, CALENDAR_READONLY],
        }
    }

    fn writing(self) -> &'static [&'static str] {
        match self {
            Self::Microsoft365 => &[MAIL_SEND, CALENDARS_WRITE],
            Self::GoogleWorkspace => &[GMAIL_SEND, CALENDAR_EVENTS],
        }
    }

    /// The permissions a person is asked when linking (ADR-119): reading
    /// always, writing too when their company lets its tools write.
    pub fn link_scopes(self, write_tools: bool) -> Vec<&'static str> {
        let mut scopes = [self.identity(), self.reading()].concat();
        if write_tools {
            scopes.extend(self.writing());
        }
        scopes
    }

    /// The permissions its tools need: reading, and writing when the
    /// company lets them write.
    pub fn tool_scopes(self, write_tools: bool) -> Vec<&'static str> {
        let mut scopes = self.reading().to_vec();
        if write_tools {
            scopes.extend(self.writing());
        }
        scopes
    }

    /// What a link's permissions say was granted, on their last segment;
    /// what a link asks without writing when the provider did not say.
    fn granted_list(self, granted: &str) -> Vec<&str> {
        let granted: Vec<&str> = granted.split_ascii_whitespace().map(last_segment).collect();
        if granted.is_empty() {
            return self
                .link_scopes(false)
                .into_iter()
                .map(last_segment)
                .collect();
        }
        granted
    }

    /// The permissions a link's refresh token is renewed with (ADR-117,
    /// ADR-119). Microsoft: the platform's, reading and writing, among those
    /// the person granted (it renews no more, and a permission the platform
    /// added since would be refused), whether or not the company still lets
    /// its tools write, so that letting them again needs no new link;
    /// offline access always, which it does not always list. Google: none
    /// named, so that it renews what the person granted.
    pub fn renewal_scopes(self, granted: &str) -> Vec<&'static str> {
        match self {
            Self::Microsoft365 => {
                let granted = self.granted_list(granted);
                self.link_scopes(true)
                    .into_iter()
                    .filter(|scope| *scope == OFFLINE_ACCESS || is_granted(&granted, scope))
                    .collect()
            }
            Self::GoogleWorkspace => Vec::new(),
        }
    }

    /// Which of the permissions a tool needs the person did not grant
    /// (ADR-119): they link their account again to grant them.
    pub fn missing_scopes<'a>(self, granted: &str, needs: &[&'a str]) -> Vec<&'a str> {
        let granted = self.granted_list(granted);
        needs
            .iter()
            .copied()
            .filter(|scope| !is_granted(&granted, scope))
            .collect()
    }

    /// What a link lets the tools do now (ADR-120): the permissions they
    /// need, writing only when the company lets them write, among those the
    /// person granted (reading when the provider did not say).
    pub fn serving_scopes(self, granted: &str, write_tools: bool) -> Vec<&'static str> {
        let needed = self.tool_scopes(write_tools);
        let missing = self.missing_scopes(granted, &needed);
        needed
            .into_iter()
            .filter(|scope| !missing.contains(scope))
            .collect()
    }

    /// The sign-in's extra parameters. Google gives a refresh token only
    /// when asked offline access, and again only when asked consent; it is
    /// asked to offer the accounts of the person's domain.
    pub fn authorization_params(self, account_email: &str) -> Vec<(&'static str, String)> {
        match self {
            Self::Microsoft365 => Vec::new(),
            Self::GoogleWorkspace => {
                let mut params = vec![
                    ("access_type", "offline".to_string()),
                    ("prompt", "consent".to_string()),
                ];
                if let Some(domain) = google::hosted_domain_hint(account_email) {
                    params.push(("hd", domain));
                }
                params
            }
        }
    }

    /// The account a verified token proves, if it is the person's own.
    pub fn linked_identity(
        self,
        claims: &TokenClaims<'_>,
        account_email: &str,
    ) -> Result<LinkedIdentity, LinkRefusal> {
        match self {
            Self::Microsoft365 => microsoft365::linked_identity(claims, account_email),
            Self::GoogleWorkspace => google::linked_identity(claims, account_email),
        }
    }
}

fn last_segment(scope: &str) -> &str {
    scope.rsplit('/').next().unwrap_or(scope)
}

fn is_granted(granted: &[&str], scope: &str) -> bool {
    let scope = last_segment(scope);
    granted
        .iter()
        .any(|granted| granted.eq_ignore_ascii_case(scope))
}
