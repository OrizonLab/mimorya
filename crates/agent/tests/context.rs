//! What the model sees: a stable system prompt, memories in the user turn.

use chrono::{TimeZone, Utc};
use mimorya_agent::runtime::{
    MemoryHit, SkillSummary, UserProfileContext, build_system_prompt, build_user_turn, french_date,
};
use mimorya_core::memory::{MemoryKind, MemoryScope};
use uuid::Uuid;

fn profile() -> UserProfileContext {
    UserProfileContext {
        assistant_name: "Mimorya".into(),
        tenant_name: "Néo Maintenance".into(),
        display_name: "Marc Lefèvre".into(),
        job_title: Some("Responsable technique".into()),
        org_unit_path: Some("Néo Maintenance › Service technique".into()),
        roles: vec!["Responsable d’équipe — Service technique et sous-unités".into()],
    }
}

#[test]
fn dates_are_french_and_in_the_paris_time_zone() {
    // 23:30 UTC on Sunday 18 October 2026 is already Monday in Paris.
    let late = Utc.with_ymd_and_hms(2026, 10, 18, 23, 30, 0).unwrap();
    assert_eq!(
        french_date(late, chrono_tz::Europe::Paris),
        "lundi 19 octobre 2026"
    );
}

#[test]
fn the_system_prompt_is_stable_within_a_day_and_lists_skills() {
    let skills = vec![SkillSummary {
        name: "synthese-hebdo".into(),
        description: "Synthèse".into(),
    }];
    let morning = build_system_prompt(
        &profile(),
        &skills,
        Utc.with_ymd_and_hms(2026, 10, 19, 7, 0, 0).unwrap(),
    );
    let evening = build_system_prompt(
        &profile(),
        &skills,
        Utc.with_ymd_and_hms(2026, 10, 19, 20, 0, 0).unwrap(),
    );
    assert_eq!(morning, evening);
    assert!(morning.contains("Tu travailles en ce moment pour Marc Lefèvre (Responsable technique, Néo Maintenance › Service technique)."));
    assert!(morning.contains("- synthese-hebdo : Synthèse"));
}

#[test]
fn memories_travel_with_the_user_turn_as_data() {
    assert_eq!(build_user_turn("Bonjour", &[]), "Bonjour");
    let id = Uuid::nil();
    let hit = MemoryHit {
        id,
        scope: MemoryScope::OrgUnit,
        kind: MemoryKind::Procedure,
        title: "Retours chantier".into(),
        content: "Le jeudi.".into(),
        org_unit_id: None,
        updated_at: Utc::now(),
        score: 1.0,
    };
    let turn = build_user_turn("Quand ?", &[hit]);
    assert!(turn.starts_with("<contexte_memoire>"));
    assert!(turn.contains(&format!(
        "- [équipe/service · procedure] Retours chantier (mémoire {id}) : Le jeudi."
    )));
    assert!(turn.ends_with("</contexte_memoire>\n\nQuand ?"));
}
