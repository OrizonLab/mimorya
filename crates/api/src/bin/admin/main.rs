//! Operator commands of a private installation (ADR-033), run with the API's
//! environment: apply the migrations without any data, create a company with
//! its first administrator, give an invited account a new link, suspend and
//! reactivate a company (ADR-191), verify the sealed audit log against the
//! seals logged (ADR-178). The link is printed for the operator to transmit,
//! and nowhere else.

mod args;
mod report;
mod verify;

use std::process::ExitCode;

use args::{Command, USAGE};
use mimorya_api::AppConfig;
use mimorya_api::mail::expiry_label;
use mimorya_api::services::provisioning::{
    CompanyStatusChange, NewCompany, OperatorInvitation, change_company_status, create_company,
    reinvite,
};
use mimorya_core::DomainError;
use mimorya_db::{Database, PoolOptions, run_migrations};

#[tokio::main]
async fn main() -> ExitCode {
    // A missing .env file is fine: the environment may come from elsewhere.
    let _ = dotenvy::dotenv();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args::parse(&args) {
        Ok(command) => command,
        Err(problem) => {
            eprintln!("{problem}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // A refusal reads as its French message; anything else with its causes.
            match error.downcast_ref::<DomainError>() {
                Some(refusal) => eprintln!("Refusé : {}", refusal.message),
                None => eprintln!("Erreur : {error:#}"),
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Help => println!("{USAGE}"),
        Command::Migrate => {
            let url = std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL (rôle propriétaire) est obligatoire"))?;
            run_migrations(&url).await?;
            println!("Migrations appliquées.");
        }
        Command::CreateCompany(company) => {
            let (db, config) = connect().await?;
            let web_origin = config.web_origin;
            let (name, slug) = (company.name.trim(), company.slug.trim());
            let named = format!("« {name} » (sigle {slug})");
            let email = company.admin_email.trim().to_lowercase();
            let created = create_company(
                &db,
                &web_origin,
                NewCompany {
                    name: company.name,
                    slug: company.slug,
                    admin_email: company.admin_email,
                    admin_name: company.admin_name,
                },
            )
            .await;
            db.close().await;
            let created = created?;
            println!(
                "Entreprise {named} créée, identifiant {}.",
                created.tenant_id
            );
            println!("Premier administrateur : {email}, invité.");
            print_link(&created.invitation, &email);
        }
        Command::Invite { email } => {
            let (db, config) = connect().await?;
            let invitation = reinvite(&db, &config.web_origin, &email).await;
            db.close().await;
            let email = email.trim().to_lowercase();
            print_link(&invitation?, &email);
        }
        Command::CompanyStatus { slug, change } => {
            let (db, _) = connect().await?;
            let changed = change_company_status(&db, &slug, change).await;
            db.close().await;
            let tenant_id = changed?;
            println!("{}", status_told(slug.trim(), tenant_id, change));
        }
        Command::VerifyAudit { anchors } => {
            let (db, config) = connect().await?;
            let verified = verify::verify_audit(&config, &db, &anchors).await;
            db.close().await;
            verified?;
        }
    }
    Ok(())
}

/// The runtime connection (row-level security applies) and the API's
/// configuration (where links lead, the master key).
async fn connect() -> anyhow::Result<(Database, AppConfig)> {
    let config = AppConfig::from_env()?;
    let db = Database::connect(
        &config.database_app_url,
        PoolOptions {
            max_connections: 1,
            application_name: "mimorya-admin",
        },
    )
    .await?;
    Ok((db, config))
}

/// What a suspension or a reactivation means, told to the operator.
fn status_told(slug: &str, tenant_id: uuid::Uuid, change: CompanyStatusChange) -> String {
    match change {
        CompanyStatusChange::Suspend => format!(
            "Entreprise de sigle {slug} suspendue (identifiant {tenant_id}) : plus personne ne \
             s’y connecte, ses sessions et ses jetons sont refusés, ses runs s’arrêtent à leur \
             prochaine étape. La commande ne supprime rien, mais les échéances des demandes \
             de validation et la rétention courent toujours. mimorya-admin \
             reactivate-company --slug {slug} la rétablit."
        ),
        CompanyStatusChange::Reactivate => format!(
            "Entreprise de sigle {slug} réactivée (identifiant {tenant_id}) : ses comptes se \
             connectent de nouveau, ses sessions encore valables et ses jetons sont acceptés."
        ),
    }
}

fn print_link(invitation: &OperatorInvitation, email: &str) {
    println!(
        "\nLien d’invitation, valable jusqu’au {} :\n{}\n",
        expiry_label(invitation.expires_at),
        invitation.link
    );
    println!(
        "Transmettez-le à {email} par un canal sûr : la personne y choisira son mot de passe. \
         Il n’est affiché qu’ici ; ne le conservez pas. Un lien remplace le précédent : \
         mimorya-admin invite --email {email}"
    );
}
