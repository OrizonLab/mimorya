-- The company's Microsoft Teams bot (ADR-138), a gateway like its Slack
-- application and its Telegram bot: one per service and company. What is
-- declared of it is kept where Slack's application keeps what Slack says:
-- its Microsoft Entra directory (tenant) as `workspace_id`, every activity
-- coming from it, and as `workspace_name`, Entra naming no directory to
-- the bot; its application (Microsoft App ID) as `bot_user_id`, every
-- activity being sent to it. Microsoft says no permissions. Its client
-- secret is sealed in the secret store, never here.
ALTER TABLE gateways
  DROP CONSTRAINT gateways_kind,
  DROP CONSTRAINT gateways_workspace_id,
  DROP CONSTRAINT gateways_bot_user_id,
  DROP CONSTRAINT gateways_telegram_scopes,
  ADD CONSTRAINT gateways_kind CHECK (kind IN ('slack', 'telegram', 'teams')),
  ADD CONSTRAINT gateways_workspace_id CHECK (
    (kind = 'slack' AND workspace_id ~ '^[TE][A-Z0-9]{2,30}$')
    OR (kind = 'telegram' AND workspace_id ~ '^[1-9][0-9]{0,19}$')
    OR (kind = 'teams'
      AND workspace_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
  ),
  ADD CONSTRAINT gateways_bot_user_id CHECK (
    (kind = 'slack' AND bot_user_id ~ '^[UW][A-Z0-9]{2,30}$')
    OR (kind = 'telegram' AND bot_user_id = workspace_id)
    OR (kind = 'teams'
      AND bot_user_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
      AND workspace_name = workspace_id)
  ),
  ADD CONSTRAINT gateways_scopes_of_slack CHECK (
    kind = 'slack' OR (granted_scopes IS NULL AND scopes_checked_at IS NULL)
  );

-- A Teams user is named by their Microsoft Entra object, and keeps the
-- address Teams gave as theirs when they linked it, as a Slack user does.
ALTER TABLE gateway_identities
  DROP CONSTRAINT gateway_identities_of_kind,
  ADD CONSTRAINT gateway_identities_of_kind CHECK (
    (kind = 'slack' AND external_id ~ '^[UW][A-Z0-9]{2,30}$'
      AND address IS NOT NULL AND display_name IS NULL)
    OR (kind = 'telegram' AND external_id ~ '^[1-9][0-9]{0,19}$'
      AND address IS NULL AND display_name IS NOT NULL)
    OR (kind = 'teams'
      AND external_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
      AND address IS NOT NULL AND display_name IS NULL)
  );

-- A run its author starts by writing to the Teams bot: `teams`. Policies
-- can read it (`context.channel`), as the others.
ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_channel_check;
ALTER TABLE agent_runs ADD CONSTRAINT agent_runs_channel_check
  CHECK (channel IN ('web', 'hook', 'slack', 'telegram', 'teams'));
