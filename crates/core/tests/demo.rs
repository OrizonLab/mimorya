//! Demo identifiers stay identical to the ones already seeded in databases.

use mimorya_core::demo::{self, demo_id, tenant, user};

#[test]
fn demo_ids_follow_the_documented_format() {
    assert_eq!(
        tenant::ID.to_string(),
        "00000000-0000-4000-8000-000000000001"
    );
    assert_eq!(
        demo_id(500).to_string(),
        "00000000-0000-4000-8000-000000000500"
    );
    assert_eq!(
        user::PAUL.to_string(),
        "00000000-0000-4000-8000-000000000209"
    );
}

#[test]
fn demo_accounts_are_consistent() {
    let users = demo::users();
    assert_eq!(users.len(), 10);
    let marc = users.iter().find(|u| u.id == user::MARC).unwrap();
    assert_eq!(marc.email, "marc.lefevre@neo-maintenance.example");
    let lea = users.iter().find(|u| u.id == user::LEA).unwrap();
    assert_eq!(lea.email, "lea.fontaine@neo-maintenance.example");
    assert!(users.iter().all(|u| u.email.is_ascii()));
}
