//! What a person has of every kind their export holds (ADR-187), written
//! as an operator writes it, each named after a mark (`de-marc`, `de-paul`,
//! `d-acme`): an export that holds another's mark has taken what is not
//! the person's. Tokens are sealed in the columns that keep them, made of
//! the mark too, so a test sees that none is ever read.

use sqlx::PgPool;
use uuid::Uuid;

use super::personal_export::mission;
use super::retention::owner;

/// What a company's people share: a connector, its standard mail, a
/// Telegram bot and an identity provider.
pub struct CompanyParts {
    pub tenant_id: Uuid,
    connector: Uuid,
    mail: Uuid,
    gateway: Uuid,
    provider: Uuid,
}

/// A person's conversation and the run made in it.
pub struct Owned {
    pub conversation: Uuid,
    pub run: Uuid,
}

/// The connector, standard mail (the company's own if it has one: one per
/// company), bot and identity provider of the company `tenant_id`; `bot` is
/// the bot's Telegram number.
pub async fn company_parts(tenant_id: Uuid, mark: &str, bot: u64) -> CompanyParts {
    let owner = owner().await;
    let connector = sqlx::query_scalar(
        "insert into connector_instances (tenant_id, type, name) \
         values ($1, 'mcp', 'connecteur-' || $2) returning id",
    )
    .bind(tenant_id)
    .bind(mark)
    .fetch_one(&owner)
    .await
    .unwrap();
    let mail = sqlx::query_scalar(
        "with found as (select id from connector_instances \
         where tenant_id = $1 and type = 'mail_server'), \
         made as (insert into connector_instances (tenant_id, type, name, config) \
         select $1, 'mail_server', 'messagerie-' || $2, \
         '{\"imap\": {\"host\": \"imap.exemple.test\", \"port\": 993}, \
         \"smtp\": {\"host\": \"smtp.exemple.test\", \"port\": 465, \"security\": \"tls\"}}' \
         where not exists (select 1 from found) returning id) \
         select id from found union all select id from made",
    )
    .bind(tenant_id)
    .bind(mark)
    .fetch_one(&owner)
    .await
    .unwrap();
    let gateway = sqlx::query_scalar(
        "insert into gateways (tenant_id, kind, name, workspace_id, workspace_name, bot_user_id) \
         values ($1, 'telegram', 'bot-' || $2, $3, 'bot-' || $2, $3) returning id",
    )
    .bind(tenant_id)
    .bind(mark)
    .bind(bot.to_string())
    .fetch_one(&owner)
    .await
    .unwrap();
    let provider = sqlx::query_scalar(
        "insert into identity_providers (tenant_id, kind, name, issuer, client_id) \
         values ($1, 'oidc', 'fournisseur-' || $2, 'https://idp.example/' || $2, 'client') \
         returning id",
    )
    .bind(tenant_id)
    .bind(mark)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    CompanyParts {
        tenant_id,
        connector,
        mail,
        gateway,
        provider,
    }
}

/// What `person` has, marked `mark`: a conversation (its two messages, its
/// summary and its Telegram thread) seen by `unit` when given, a personal memory, a task, a
/// trigger, a mission, a run with its tool call, validation, deliverable
/// and consumption (`tokens` in, half as many out), a session, a linked
/// account, an account at the standard mail, a server authorization, a
/// Telegram identity (`telegram`), a sign-in and a SCIM account.
pub async fn belongings(
    parts: &CompanyParts,
    person: Uuid,
    mark: &str,
    unit: Option<Uuid>,
    telegram: u64,
    tokens: i32,
) -> Owned {
    let owner = owner().await;
    let tenant_id = parts.tenant_id;
    let conversation = conversation(&owner, tenant_id, person, mark, unit).await;
    mission(tenant_id, person, conversation, &format!("mission-{mark}")).await;
    let run = run(&owner, tenant_id, conversation, person, mark, tokens).await;
    let statements = [
        "insert into memory_items (tenant_id, scope, owner_user_id, kind, title, content, source) \
         values ($1, 'user', $2, 'preference', 'mémoire-' || $3, 'contenu-' || $3, '{}')",
        "insert into scheduled_tasks (tenant_id, user_id, title, instruction, weekdays, \
         time_of_day, time_zone) values ($1, $2, 'tâche-' || $3, 'consigne-' || $3, '{1}', \
         '08:30', 'Europe/Paris')",
        "insert into inbound_hooks (tenant_id, user_id, title, instruction, token_hash) \
         values ($1, $2, 'déclencheur-' || $3, 'consigne-' || $3, \
         encode(sha256(convert_to($3, 'UTF8')), 'hex'))",
        "insert into sessions (tenant_id, user_id, token_hash, expires_at, absolute_expires_at, \
         ip, user_agent) values ($1, $2, 'hash-' || $3, now() + interval '1 day', \
         now() + interval '7 days', '192.0.2.7', 'navigateur-' || $3)",
        "insert into scim_accounts (tenant_id, user_id, external_id) values ($1, $2, 'scim-' || $3)",
    ];
    for statement in statements {
        sqlx::query(statement)
            .bind(tenant_id)
            .bind(person)
            .bind(mark)
            .execute(&owner)
            .await
            .unwrap();
    }
    links(&owner, parts, person, mark, telegram).await;
    // The conversation is held in the person’s chat with the bot.
    sqlx::query(
        "insert into gateway_threads (tenant_id, gateway_id, user_id, conversation_id, kind, \
         external_id, channel_id, thread_ts, created_at, active_at) \
         values ($1, $2, $3, $4, 'telegram', $5, $5, '1', now(), now())",
    )
    .bind(tenant_id)
    .bind(parts.gateway)
    .bind(person)
    .bind(conversation)
    .bind(telegram.to_string())
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    Owned { conversation, run }
}

/// An invitation awaiting `person`, sent by `inviter`; its token's hash is
/// the SHA-256 of `mark`, in hexadecimal.
pub async fn invited(tenant_id: Uuid, person: Uuid, inviter: Option<Uuid>, mark: &str) {
    let owner = owner().await;
    sqlx::query(
        "insert into user_invitations (tenant_id, user_id, token_hash, created_by_user_id, \
         expires_at) values ($1, $2, encode(sha256(convert_to($4, 'UTF8')), 'hex'), $3, \
         now() + interval '7 days')",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(inviter)
    .bind(mark)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

async fn conversation(
    owner: &PgPool,
    tenant_id: Uuid,
    person: Uuid,
    mark: &str,
    unit: Option<Uuid>,
) -> Uuid {
    let id = sqlx::query_scalar(
        "insert into conversations (tenant_id, owner_user_id, title, visibility, org_unit_id) \
         values ($1, $2, 'conversation-' || $3, \
         case when $4::uuid is null then 'private' else 'org_unit' end, $4) returning id",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(mark)
    .bind(unit)
    .fetch_one(owner)
    .await
    .unwrap();
    sqlx::query(
        "with written as (insert into messages (tenant_id, conversation_id, role, author_user_id, \
         content, created_at) values ($1, $2, 'user', $3, 'question-' || $4, now()), \
         ($1, $2, 'assistant', null, 'réponse-' || $4, now() + interval '1 second') \
         returning id, role) \
         insert into conversation_summaries (tenant_id, conversation_id, content, \
         through_message_id, folded_messages) \
         select $1, $2, 'résumé-' || $4, id, 1 from written where role = 'user'",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(person)
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
    id
}

async fn run(
    owner: &PgPool,
    tenant_id: Uuid,
    conversation: Uuid,
    person: Uuid,
    mark: &str,
    tokens: i32,
) -> Uuid {
    let run = sqlx::query_scalar(
        "insert into agent_runs (tenant_id, conversation_id, initiated_by_user_id, \
         on_behalf_of_user_id, objective, budget, status, finished_at) \
         values ($1, $2, $3, $3, 'objectif-' || $4, '{}', 'completed', now()) returning id",
    )
    .bind(tenant_id)
    .bind(conversation)
    .bind(person)
    .bind(mark)
    .fetch_one(owner)
    .await
    .unwrap();
    sqlx::query(
        "with called as (insert into tool_invocations (tenant_id, run_id, tool_call_id, \
         tool_name, input, decision, status, output_summary) values ($1, $2, 'appel-1', \
         'outil-' || $4, jsonb_build_object('note', 'entrée-' || $4), 'require_approval', \
         'succeeded', 'sortie-' || $4)), \
         asked as (insert into approvals (tenant_id, run_id, requested_for_user_id, tool_name, \
         tool_call_id, action_summary, payload, risk, reason, status, decided_by_user_id, \
         decided_at, decision_comment) values ($1, $2, $3, 'outil-' || $4, 'appel-1', \
         'action-' || $4, jsonb_build_object('note', 'charge-' || $4), 'write', \
         'raison-' || $4, 'approved', $3, now(), 'commentaire-' || $4)), \
         delivered as (insert into deliverables (tenant_id, run_id, owner_user_id, type, title, \
         content) values ($1, $2, $3, 'report', 'livrable-' || $4, \
         jsonb_build_object('texte', 'contenu-' || $4))) \
         insert into usage_records (tenant_id, user_id, run_id, kind, provider, model, \
         input_tokens, output_tokens, cost_micros) \
         values ($1, $3, $2, 'llm', 'anthropic', 'modèle', $5, $5 / 2, $5 * 10)",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(person)
    .bind(mark)
    .bind(tokens)
    .execute(owner)
    .await
    .unwrap();
    run
}

/// The person's linked account, account at the standard mail, server
/// authorization, Telegram identity and sign-in, through the company's
/// parts.
async fn links(owner: &PgPool, parts: &CompanyParts, person: Uuid, mark: &str, telegram: u64) {
    sqlx::query(
        "insert into mail_accounts (tenant_id, connector_id, user_id, username, \
         password_ciphertext, password_iv, password_tag, key_version) \
         values ($1, $2, $3, 'compte-' || $4, convert_to('chiffré-' || $4, 'UTF8'), '\\x00', \
         '\\x00', 1)",
    )
    .bind(parts.tenant_id)
    .bind(parts.mail)
    .bind(person)
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into account_links (tenant_id, connector_id, user_id, subject, address, scopes, \
         refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version) \
         values ($1, $2, $3, 'sujet-' || $4, $4 || '@exemple.test', 'Mail.Read', \
         convert_to('chiffré-' || $4, 'UTF8'), '\\x00', '\\x00', 1)",
    )
    .bind(parts.tenant_id)
    .bind(parts.connector)
    .bind(person)
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into connector_person_authorizations (tenant_id, connector_id, user_id, issuer, \
         client_id, resource, scopes, access_ciphertext, access_iv, access_auth_tag, \
         access_key_version) values ($1, $2, $3, 'https://as.example', 'client-' || $4, \
         'https://mcp.example', 'portée-' || $4, convert_to('accès-' || $4, 'UTF8'), '\\x00', \
         '\\x00', 1)",
    )
    .bind(parts.tenant_id)
    .bind(parts.connector)
    .bind(person)
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into gateway_identities (tenant_id, gateway_id, user_id, kind, external_id, \
         display_name) values ($1, $2, $3, 'telegram', $4, 'nom-' || $5)",
    )
    .bind(parts.tenant_id)
    .bind(parts.gateway)
    .bind(person)
    .bind(telegram.to_string())
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into user_identities (tenant_id, provider_id, user_id, subject) \
         values ($1, $2, $3, 'sso-' || $4)",
    )
    .bind(parts.tenant_id)
    .bind(parts.provider)
    .bind(person)
    .bind(mark)
    .execute(owner)
    .await
    .unwrap();
}

/// A validation asked for `requester` on their run `run`, which `decider`
/// gave, marked `mark`.
pub async fn decided_for(tenant_id: Uuid, run: Uuid, requester: Uuid, decider: Uuid, mark: &str) {
    let owner = owner().await;
    sqlx::query(
        "insert into approvals (tenant_id, run_id, requested_for_user_id, tool_name, \
         tool_call_id, action_summary, payload, risk, reason, status, decided_by_user_id, \
         decided_at, decision_comment) values ($1, $2, $3, 'outil-décidé', 'appel-2', \
         'action-' || $5, jsonb_build_object('note', 'charge-' || $5), 'write', \
         'raison-' || $5, 'rejected', $4, now(), 'commentaire-' || $5)",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(requester)
    .bind(decider)
    .bind(mark)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
