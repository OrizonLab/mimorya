//! The rows a deleted conversation takes with it, or empties, are found
//! through an index (ADR-186). The retention pass deletes conversations,
//! their runs and events, messages and validations, their missions and
//! threads; each key that deletes or empties rows with them is looked up
//! once per row deleted. Without an index starting with its columns, each
//! lookup would read all of the company's rows.

mod support;

use mimorya_db::Database;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

/// The keys into the purged tables that delete or empty their rows, with
/// no valid index whose first columns are theirs, or that starts with the
/// column other than the company. An index counts when it is a b-tree or a
/// hash, on its key columns alone (not those it merely includes), whole or
/// partial over the rows where one of the key's columns is set: a lookup
/// names it.
const UNINDEXED_KEYS: &str = "\
    select c.conname::text from pg_constraint c \
    where c.contype = 'f' and c.connamespace = 'public'::regnamespace \
      and c.confdeltype in ('c', 'n') \
      and c.confrelid in ('conversations'::regclass, 'agent_runs'::regclass, \
                          'messages'::regclass, 'approvals'::regclass, \
                          'gateway_threads'::regclass, 'missions'::regclass, \
                          'run_events'::regclass) \
      and not exists ( \
        select 1 from pg_index i \
        where i.indrelid = c.conrelid \
          and i.indisvalid \
          and (select am.amname from pg_class ic join pg_am am on am.oid = ic.relam \
                where ic.oid = i.indexrelid) in ('btree', 'hash') \
          and (i.indpred is null \
               or pg_get_expr(i.indpred, i.indrelid) in ( \
                 select format('(%s IS NOT NULL)', quote_ident(a.attname)) \
                 from pg_attribute a \
                 where a.attrelid = c.conrelid and a.attnum = any (c.conkey))) \
          and ((cardinality(c.conkey) <= i.indnkeyatts \
                and (select array_agg(k order by k) from unnest(c.conkey) k) \
                    = (select array_agg(a order by a) \
                         from unnest((i.indkey::int2[])[0:cardinality(c.conkey) - 1]) a)) \
               or (i.indkey[0] = any (c.conkey) \
                   and i.indkey[0] <> (select attnum from pg_attribute \
                                        where attrelid = c.conrelid \
                                          and attname = 'tenant_id')))) \
    order by 1";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_key_a_purge_follows_has_its_index() {
    let db = app().await;
    let unindexed: Vec<String> = sqlx::query_scalar(UNINDEXED_KEYS)
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert!(unindexed.is_empty(), "keys without an index: {unindexed:?}");

    // The check looks at the schema's keys, not at none of them.
    let followed: i64 = sqlx::query_scalar(
        "select count(*) from pg_constraint c \
         where c.contype = 'f' and c.confdeltype in ('c', 'n') \
           and c.confrelid in ('conversations'::regclass, 'agent_runs'::regclass, \
                               'messages'::regclass)",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(followed >= 15, "{followed} keys followed");
}
