//! The command line of `mimorya-admin`: a command, then its options as
//! `--option value` pairs, each given once.

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
  verify-audit     vérifie le journal d’audit scellé de chaque entreprise (MIMORYA_MASTER_KEY) ;
                   échoue si une entrée a été modifiée ou supprimée
  help             affiche cette aide

create-company, invite et verify-audit passent par DATABASE_APP_URL ; les liens mènent à
WEB_ORIGIN.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Migrate,
    CreateCompany(CompanyArgs),
    Invite { email: String },
    VerifyAudit,
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
        "verify-audit" => Command::VerifyAudit,
        other => return Err(format!("Commande inconnue : {other}")),
    };
    options.finish()?;
    Ok(parsed)
}

struct Options(Vec<(String, String)>);

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
            if pairs.iter().any(|(given, _)| given == name) {
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
        assert_eq!(parse_line(&["verify-audit"]), Ok(Command::VerifyAudit));
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
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        let refused: [&[&str]; 8] = [
            &["deploy"],
            &["invite"],
            &["invite", "--email"],
            &["invite", "jeanne@acme.example"],
            &["invite", "--email", "a@b.example", "--email", "c@d.example"],
            &["invite", "--email", "a@b.example", "--name", "Acme"],
            &["migrate", "--force", "yes"],
            &["verify-audit", "--company", "acme"],
        ];
        for line in refused {
            assert!(parse_line(line).is_err(), "{line:?}");
        }
    }
}
