//! Demo documents (ADR-095, ADR-096), deposited as Markdown files and cut
//! into their passages as a deposit would: a quality charter in the
//! company's space, read by all, and a maintenance procedure in the
//! technical department's, read by its teams and its management.

use mimorya_core::demo::{tenant, unit, user};
use mimorya_core::documents::{DocumentSpace, Extracted, passages, title_of};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::repositories::documents::{NewDocument, insert_document, insert_passages};

struct DemoDocument {
    file_name: &'static str,
    space: DocumentSpace,
    author: Uuid,
    text: &'static str,
}

const DOCUMENTS: &[DemoDocument] = &[
    DemoDocument {
        file_name: "Charte qualité des interventions.md",
        space: DocumentSpace::Company,
        author: user::CLAIRE,
        text: "# Charte qualité des interventions\n\n\
            Néo Maintenance s’engage auprès de ses clients sur des interventions ponctuelles, \
            propres et tracées.\n\n\
            ## Délais\n\n\
            Une panne de chauffage signalée avant 12 h est prise en charge le jour même. Les \
            autres demandes sont planifiées sous cinq jours ouvrés.\n\n\
            ## Compte rendu\n\n\
            Chaque intervention se termine par un compte rendu complet : travaux réalisés, \
            pièces remplacées, mesures relevées. Une intervention dont une pièce est en attente \
            reste « en cours » jusqu’à la pose de la pièce.\n\n\
            ## Signature du client\n\n\
            Le client signe le compte rendu sur la tablette. En son absence, le technicien \
            photographie l’installation et le note dans le compte rendu.\n",
    },
    DemoDocument {
        file_name: "Entretien annuel d’une chaudière fioul.md",
        space: DocumentSpace::Unit(unit::TECHNICAL),
        author: user::MARC,
        text: "# Entretien annuel d’une chaudière fioul\n\n\
            ## Préparation\n\n\
            Couper l’alimentation électrique et fermer la vanne fioul. Laisser refroidir le \
            corps de chauffe au moins trente minutes.\n\n\
            ## Nettoyage\n\n\
            Démonter le brûleur, brosser le foyer et les carneaux, aspirer les suies. Remplacer \
            le gicleur et le filtre fioul.\n\n\
            ## Remise en service\n\n\
            Rouvrir la vanne, remettre sous tension, purger la pompe fioul. Contrôler la \
            combustion à l’analyseur : le taux de CO doit rester sous 50 ppm. Consigner les \
            mesures dans le compte rendu.\n",
    },
];

pub async fn insert(conn: &mut PgConnection) -> anyhow::Result<()> {
    for document in DOCUMENTS {
        let id = insert_document(
            conn,
            NewDocument {
                tenant_id: tenant::ID,
                scope: document.space.scope(),
                org_unit_id: document.space.org_unit_id(),
                title: &title_of(document.file_name),
                file_name: document.file_name,
                kind: "markdown",
                sensitivity: "internal",
                page_count: None,
                uploaded_by_user_id: document.author,
                content: document.text.as_bytes(),
            },
        )
        .await?;
        let read = Extracted {
            pages: vec![document.text.to_string()],
            paged: false,
        };
        insert_passages(conn, tenant::ID, id, &passages(&read)).await?;
    }
    Ok(())
}
