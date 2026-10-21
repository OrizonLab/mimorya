//! The only functions that read or write across companies (`auth_*`,
//! `system_*`) run as their owner (`SECURITY DEFINER`) with a fixed search
//! path, and only the application role may call them: nobody through
//! `PUBLIC`. A new one must join the list knowingly.

mod support;

const EXPECTED: [&str; 8] = [
    "auth_find_invitation",
    "auth_find_login",
    "auth_find_password_reset",
    "auth_resolve_session",
    "system_fail_stale_runs",
    "system_interrupted_runs",
    "system_overdue_approval_runs",
    "system_purge_expired_password_resets",
];

#[derive(sqlx::FromRow)]
struct DefinerFunction {
    name: String,
    search_path: Option<Vec<String>>,
    public_may_execute: bool,
    app_may_execute: bool,
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_known_definer_functions_exist_and_only_the_application_calls_them() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let functions: Vec<DefinerFunction> = sqlx::query_as(
        "select p.proname::text as name, p.proconfig as search_path, \
         (p.proacl is null or exists (select 1 from aclexplode(p.proacl) a \
            where a.grantee = 0 and a.privilege_type = 'EXECUTE')) as public_may_execute, \
         has_function_privilege('mimorya_app', p.oid, 'EXECUTE') as app_may_execute \
         from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
         where n.nspname = 'public' and p.prosecdef order by 1",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, EXPECTED);
    for function in &functions {
        assert_eq!(
            function.search_path.as_deref(),
            Some(&["search_path=public, pg_temp".to_string()][..]),
            "{}",
            function.name
        );
        assert!(
            !function.public_may_execute,
            "{} callable by PUBLIC",
            function.name
        );
        assert!(
            function.app_may_execute,
            "{} not callable by the application",
            function.name
        );
    }
}
