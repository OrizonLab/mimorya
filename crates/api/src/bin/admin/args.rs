//! The command line of `mimorya-admin`: a command, then its options as
//! `--option value` pairs, each given once but `--anchor`.

use mimorya_api::services::audit_chain::Anchor;
use mimorya_api::services::provisioning::CompanyStatusChange;

use super::supply::{self, SupplyCommand};
pub use super::usage::USAGE;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Migrate,
    CreateCompany(CompanyArgs),
    Invite {
        email: String,
    },
    CompanyStatus {
        slug: String,
        change: CompanyStatusChange,
    },
    ReleaseCompany {
        slug: String,
    },
    DeleteCompany {
        slug: String,
        confirm: Option<String>,
    },
    VerifyAudit {
        anchors: Vec<Anchor>,
    },
    /// How a company's model is supplied (ADR-208).
    Supply(SupplyCommand),
    /// What the installation has ready and what it lacks (ADR-223).
    CheckInstall {
        mail_to: Option<String>,
        call_model: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct CompanyArgs {
    pub name: String,
    pub slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

/// The command `args` (program name excluded) asks for, or what is wrong.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    let mut options = Options::read(rest)?;
    let parsed = match command.as_str() {
        "help" | "--help" | "-h" => Command::Help,
        "migrate" => Command::Migrate,
        "create-company" => Command::CreateCompany(CompanyArgs {
            name: options.take("--name")?,
            slug: options.take("--slug")?,
            admin_email: options.take("--admin-email")?,
            admin_name: options.take("--admin-name")?,
        }),
        "invite" => Command::Invite {
            email: options.take("--email")?,
        },
        "suspend-company" => Command::CompanyStatus {
            slug: options.take("--slug")?,
            change: CompanyStatusChange::Suspend,
        },
        "reactivate-company" => Command::CompanyStatus {
            slug: options.take("--slug")?,
            change: CompanyStatusChange::Reactivate,
        },
        "release-company" => Command::ReleaseCompany {
            slug: options.take("--slug")?,
        },
        "delete-company" => Command::DeleteCompany {
            slug: options.take("--slug")?,
            confirm: options.take_optional("--confirm"),
        },
        "verify-audit" => Command::VerifyAudit {
            anchors: options
                .take_all("--anchor")
                .iter()
                .map(|anchor| Anchor::parse(anchor))
                .collect::<Result<_, _>>()?,
        },
        "check-install" => Command::CheckInstall {
            mail_to: options.take_optional("--mail-to"),
            call_model: options.take_flag("--call-model"),
        },
        command if supply::COMMANDS.contains(&command) => {
            Command::Supply(supply::parse(command, &mut options)?)
        }
        other => return Err(format!("Commande inconnue : {other}")),
    };
    options.finish()?;
    Ok(parsed)
}

pub struct Options(Vec<(String, String)>);

/// The only option given as many times as wanted.
const REPEATED: &str = "--anchor";

/// The options given alone, without a value.
const FLAGS: [&str; 1] = ["--call-model"];

impl Options {
    pub fn read(args: &[String]) -> Result<Self, String> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut args = args.iter();
        while let Some(name) = args.next() {
            if !name.starts_with("--") {
                return Err(format!("Option attendue (--nom valeur), reçu : {name}"));
            }
            if FLAGS.contains(&name.as_str()) {
                if pairs.iter().any(|(given, _)| given == name) {
                    return Err(format!("Option donnée deux fois : {name}"));
                }
                pairs.push((name.clone(), String::new()));
                continue;
            }
            let Some(value) = args.next() else {
                return Err(format!("Valeur manquante pour {name}"));
            };
            if name != REPEATED && pairs.iter().any(|(given, _)| given == name) {
                return Err(format!("Option donnée deux fois : {name}"));
            }
            pairs.push((name.clone(), value.clone()));
        }
        Ok(Self(pairs))
    }

    pub fn take(&mut self, name: &str) -> Result<String, String> {
        match self.0.iter().position(|(given, _)| given == name) {
            Some(index) => Ok(self.0.remove(index).1),
            None => Err(format!("Option obligatoire : {name}")),
        }
    }

    /// The value given to `name`, if any.
    pub fn take_optional(&mut self, name: &str) -> Option<String> {
        let index = self.0.iter().position(|(given, _)| given == name)?;
        Some(self.0.remove(index).1)
    }

    /// Every value given to `name`, in their order.
    fn take_all(&mut self, name: &str) -> Vec<String> {
        let (taken, kept) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|(given, _)| given == name);
        self.0 = kept;
        taken.into_iter().map(|(_, value)| value).collect()
    }

    /// Whether the flag `name` was given.
    pub fn take_flag(&mut self, name: &str) -> bool {
        self.take_optional(name).is_some()
    }

    /// Refuses the options the command does not know.
    pub fn finish(self) -> Result<(), String> {
        match self.0.first() {
            Some((name, _)) => Err(format!("Option inconnue pour cette commande : {name}")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line(line: &[&str]) -> Result<Command, String> {
        parse(
            &line
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn reads_each_command_and_its_options() {
        assert_eq!(parse_line(&[]), Ok(Command::Help));
        assert_eq!(parse_line(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_line(&["migrate"]), Ok(Command::Migrate));
        assert_eq!(
            parse_line(&["check-install"]),
            Ok(Command::CheckInstall {
                mail_to: None,
                call_model: false
            })
        );
        assert_eq!(
            parse_line(&[
                "check-install",
                "--call-model",
                "--mail-to",
                "ops@example.com"
            ]),
            Ok(Command::CheckInstall {
                mail_to: Some("ops@example.com".into()),
                call_model: true
            })
        );
        assert_eq!(
            parse_line(&["check-install", "--call-model", "--call-model"]),
            Err("Option donnée deux fois : --call-model".into())
        );
        assert_eq!(
            parse_line(&["verify-audit"]),
            Ok(Command::VerifyAudit {
                anchors: Vec::new()
            })
        );
        let (one, two) = (
            format!("acme:3:{}", "ab".repeat(32)),
            format!("beta:9:{}", "cd".repeat(32)),
        );
        let Ok(Command::VerifyAudit { anchors }) =
            parse_line(&["verify-audit", "--anchor", &one, "--anchor", &two])
        else {
            panic!("anchors not read");
        };
        let read: Vec<(&str, i64)> = anchors
            .iter()
            .map(|a| (a.company.as_str(), a.seq))
            .collect();
        assert_eq!(read, [("acme", 3), ("beta", 9)]);
        assert_eq!(
            parse_line(&[
                "create-company",
                "--slug",
                "acme",
                "--name",
                "Acme Industrie",
                "--admin-name",
                "Jeanne Martin",
                "--admin-email",
                "jeanne@acme.example",
            ]),
            Ok(Command::CreateCompany(CompanyArgs {
                name: "Acme Industrie".into(),
                slug: "acme".into(),
                admin_email: "jeanne@acme.example".into(),
                admin_name: "Jeanne Martin".into(),
            }))
        );
        assert_eq!(
            parse_line(&["invite", "--email", "jeanne@acme.example"]),
            Ok(Command::Invite {
                email: "jeanne@acme.example".into()
            })
        );
        assert_eq!(
            parse_line(&["suspend-company", "--slug", "acme"]),
            Ok(Command::CompanyStatus {
                slug: "acme".into(),
                change: CompanyStatusChange::Suspend
            })
        );
        assert_eq!(
            parse_line(&["release-company", "--slug", "acme"]),
            Ok(Command::ReleaseCompany {
                slug: "acme".into()
            })
        );
        assert_eq!(
            parse_line(&["delete-company", "--slug", "acme"]),
            Ok(Command::DeleteCompany {
                slug: "acme".into(),
                confirm: None
            })
        );
        assert_eq!(
            parse_line(&["delete-company", "--confirm", "acme", "--slug", "acme"]),
            Ok(Command::DeleteCompany {
                slug: "acme".into(),
                confirm: Some("acme".into())
            })
        );
        assert_eq!(
            parse_line(&["reactivate-company", "--slug", "acme"]),
            Ok(Command::CompanyStatus {
                slug: "acme".into(),
                change: CompanyStatusChange::Reactivate
            })
        );
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        let refused: [&[&str]; 17] = [
            &["deploy"],
            &["invite"],
            &["invite", "--email"],
            &["invite", "jeanne@acme.example"],
            &["invite", "--email", "a@b.example", "--email", "c@d.example"],
            &["invite", "--email", "a@b.example", "--name", "Acme"],
            &["migrate", "--force", "yes"],
            &["verify-audit", "--company", "acme"],
            &["verify-audit", "--anchor", "acme:3:abcd"],
            &["suspend-company"],
            &["suspend-company", "--slug", "acme", "--slug", "beta"],
            &["reactivate-company", "--name", "Acme"],
            &["delete-company", "--confirm", "acme"],
            &[
                "delete-company",
                "--slug",
                "a",
                "--confirm",
                "a",
                "--confirm",
                "b",
            ],
            &["delete-company", "--slug", "acme", "--force", "yes"],
            &["release-company"],
            &["release-company", "--slug", "acme", "--confirm", "acme"],
        ];
        for line in refused {
            assert!(parse_line(line).is_err(), "{line:?}");
        }
    }
}
