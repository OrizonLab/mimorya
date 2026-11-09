//! The command line of `mimorya-admin`: a command, then its options as
//! `--option value` pairs, each given once but `--anchor`.

use mimorya_api::services::audit_chain::Anchor;
use mimorya_api::services::provisioning::CompanyStatusChange;

pub const USAGE: &str = "\
Usage : mimorya-admin <commande> [options]

Commandes :
  migrate          applique les migrations de la base (DATABASE_URL, rôle propriétaire),
                   sans aucune donnée
  create-company   crée une entreprise et son premier administrateur, invité par un lien
                     --name <nom>  --slug <sigle>  --admin-email <adresse>  --admin-name <nom>
  invite           nouveau lien d’invitation d’un compte qui n’a pas encore choisi
                   son mot de passe
                     --email <adresse>
  suspend-company  suspend une entreprise : plus personne ne s’y connecte, ses sessions et
                   ses jetons sont refusés, ses runs s’arrêtent à leur prochaine étape ;
                   rien n’est supprimé
                     --slug <sigle>
  reactivate-company
                   réactive une entreprise suspendue, retrouvée telle qu’elle était
                     --slug <sigle>
  verify-audit     vérifie le journal d’audit scellé de chaque entreprise (MIMORYA_MASTER_KEY) ;
                   échoue si une entrée scellée a été modifiée ou supprimée, ou si une
                   chaîne ne porte plus un sceau relevé dans les journaux (« Audit log
                   sealed », « Audit chain found ») : seul ce sceau montre des entrées
                   retirées de la fin d’une chaîne, ou des sceaux posés à nouveau
                     [--anchor <entreprise>:<place>:<sceau>]…   (autant que voulu ;
                     l’entreprise par son identifiant, tel que journalisé, ou son sigle)
  help             affiche cette aide

create-company, invite, suspend-company, reactivate-company et verify-audit passent par
DATABASE_APP_URL ; les liens mènent à WEB_ORIGIN.";

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
    VerifyAudit {
        anchors: Vec<Anchor>,
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
        "verify-audit" => Command::VerifyAudit {
            anchors: options
                .take_all("--anchor")
                .iter()
                .map(|anchor| Anchor::parse(anchor))
                .collect::<Result<_, _>>()?,
        },
        other => return Err(format!("Commande inconnue : {other}")),
    };
    options.finish()?;
    Ok(parsed)
}

struct Options(Vec<(String, String)>);

/// The only option given as many times as wanted.
const REPEATED: &str = "--anchor";

impl Options {
    fn read(args: &[String]) -> Result<Self, String> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut args = args.iter();
        while let Some(name) = args.next() {
            if !name.starts_with("--") {
                return Err(format!("Option attendue (--nom valeur), reçu : {name}"));
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

    fn take(&mut self, name: &str) -> Result<String, String> {
        match self.0.iter().position(|(given, _)| given == name) {
            Some(index) => Ok(self.0.remove(index).1),
            None => Err(format!("Option obligatoire : {name}")),
        }
    }

    /// Every value given to `name`, in their order.
    fn take_all(&mut self, name: &str) -> Vec<String> {
        let (taken, kept) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|(given, _)| given == name);
        self.0 = kept;
        taken.into_iter().map(|(_, value)| value).collect()
    }

    /// Refuses the options the command does not know.
    fn finish(self) -> Result<(), String> {
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
            parse_line(&["reactivate-company", "--slug", "acme"]),
            Ok(Command::CompanyStatus {
                slug: "acme".into(),
                change: CompanyStatusChange::Reactivate
            })
        );
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        let refused: [&[&str]; 12] = [
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
        ];
        for line in refused {
            assert!(parse_line(line).is_err(), "{line:?}");
        }
    }
}
