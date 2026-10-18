//! Demo memories, one per memory space, so the visibility rules can be seen at
//! work: team and department procedures, a company fact, personal preferences.

use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::repositories::memory::{MemorySource, NewMemory, create_memory};

struct DemoMemory {
    scope: MemoryScope,
    owner: Option<Uuid>,
    unit: Option<Uuid>,
    kind: MemoryKind,
    title: &'static str,
    content: &'static str,
    author: Uuid,
}

const MEMORIES: &[DemoMemory] = &[
    DemoMemory {
        scope: MemoryScope::OrgUnit,
        owner: None,
        unit: Some(unit::TEAM_NORTH),
        kind: MemoryKind::Procedure,
        title: "Retours chantier équipe Nord",
        content: "Les retours sur chantier de l’équipe Nord sont planifiés le jeudi après-midi. Prévoir 2 h par intervention avec pièce à remplacer.",
        author: user::JULIEN,
    },
    DemoMemory {
        scope: MemoryScope::OrgUnit,
        owner: None,
        unit: Some(unit::TECHNICAL),
        kind: MemoryKind::Procedure,
        title: "Clôture d’une intervention",
        content: "Une intervention n’est terminée que si le compte rendu est signé par le client et qu’aucune pièce n’est en attente.",
        author: user::MARC,
    },
    DemoMemory {
        scope: MemoryScope::Tenant,
        owner: None,
        unit: None,
        kind: MemoryKind::Fact,
        title: "Horaires d’intervention",
        content: "Les interventions sont planifiées du lundi au vendredi de 8 h à 17 h. L’astreinte du week-end est organisée par la direction.",
        author: user::CLAIRE,
    },
    DemoMemory {
        scope: MemoryScope::User,
        owner: Some(user::MARC),
        unit: None,
        kind: MemoryKind::Preference,
        title: "Format des synthèses",
        content: "Marc préfère des synthèses courtes, sous forme de tableau trié par priorité.",
        author: user::MARC,
    },
    DemoMemory {
        scope: MemoryScope::User,
        owner: Some(user::LUCAS),
        unit: None,
        kind: MemoryKind::Preference,
        title: "Prévenance des déplacements",
        content: "Lucas souhaite être prévenu la veille de ses déplacements hors secteur.",
        author: user::LUCAS,
    },
];

pub(super) async fn insert(conn: &mut PgConnection) -> sqlx::Result<()> {
    for memory in MEMORIES {
        create_memory(
            &mut *conn,
            NewMemory {
                tenant_id: tenant::ID,
                scope: memory.scope,
                owner_user_id: memory.owner,
                org_unit_id: memory.unit,
                kind: memory.kind,
                title: memory.title.into(),
                content: memory.content.into(),
                sensitivity: Sensitivity::Internal,
                status: "active",
                source: MemorySource::manual(Some("Données de démonstration")),
                created_by_user_id: Some(memory.author),
                created_by_run_id: None,
            },
        )
        .await?;
    }
    Ok(())
}
