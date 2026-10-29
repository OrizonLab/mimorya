-- The company's Telegram bot (ADR-133), a gateway like its Slack
-- application: one per service and company. What Telegram says of the bot
-- is kept where Slack's application keeps what Slack says: its identifier
-- as `workspace_id` and `bot_user_id` (every update is its own), its
-- username as `workspace_name`. Telegram says no permissions. Its token
-- and its webhook's secret are sealed in the secret store, never here.
ALTER TABLE gateways
  DROP CONSTRAINT gateways_kind,
  DROP CONSTRAINT gateways_workspace_id,
  DROP CONSTRAINT gateways_bot_user_id,
  ADD CONSTRAINT gateways_kind CHECK (kind IN ('slack', 'telegram')),
  ADD CONSTRAINT gateways_workspace_id CHECK (
    (kind = 'slack' AND workspace_id ~ '^[TE][A-Z0-9]{2,30}$')
    OR (kind = 'telegram' AND workspace_id ~ '^[1-9][0-9]{0,19}$')
  ),
  ADD CONSTRAINT gateways_bot_user_id CHECK (
    (kind = 'slack' AND bot_user_id ~ '^[UW][A-Z0-9]{2,30}$')
    OR (kind = 'telegram' AND bot_user_id = workspace_id)
  ),
  ADD CONSTRAINT gateways_telegram_scopes CHECK (
    kind <> 'telegram' OR (granted_scopes IS NULL AND scopes_checked_at IS NULL)
  );

-- A gateway's identities and codes are of its service, which the database
-- holds them to: each names its gateway with its service.
ALTER TABLE gateways ADD CONSTRAINT gateways_id_kind UNIQUE (id, kind);

-- A Telegram user is named by their identifier and shown by their name:
-- Telegram proves no address, so none is kept for them. A Slack user keeps
-- their address and Slack's identifier, as before.
ALTER TABLE gateway_identities
  ADD COLUMN kind text,
  ADD COLUMN display_name text;

UPDATE gateway_identities i SET kind = g.kind FROM gateways g WHERE g.id = i.gateway_id;

ALTER TABLE gateway_identities
  ALTER COLUMN kind SET NOT NULL,
  ALTER COLUMN address DROP NOT NULL,
  DROP CONSTRAINT gateway_identities_external_id,
  ADD CONSTRAINT gateway_identities_gateway_kind FOREIGN KEY (gateway_id, kind)
    REFERENCES gateways (id, kind) ON DELETE CASCADE,
  ADD CONSTRAINT gateway_identities_display_name CHECK (
    display_name IS NULL OR char_length(display_name) BETWEEN 1 AND 80
  ),
  ADD CONSTRAINT gateway_identities_of_kind CHECK (
    (kind = 'slack' AND external_id ~ '^[UW][A-Z0-9]{2,30}$'
      AND address IS NOT NULL AND display_name IS NULL)
    OR (kind = 'telegram' AND external_id ~ '^[1-9][0-9]{0,19}$'
      AND address IS NULL AND display_name IS NOT NULL)
  );

-- The codes people link their Telegram account with (ADR-133): asked for
-- in their own session, kept as their digest only, valid fifteen minutes.
-- Once Telegram sent one back from a person's private chat with the bot,
-- the bot names whose account it links and that Telegram user is kept as
-- the one it was offered to; once they accept, they are kept as the one it
-- was sent back from, until the session that asked for it confirms the
-- link, which takes it. One per person and gateway, of a Telegram bot;
-- forgotten once expired.
CREATE TABLE gateway_link_codes (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  gateway_id uuid NOT NULL,
  kind text NOT NULL DEFAULT 'telegram',
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  code_hash text NOT NULL,
  expires_at timestamptz NOT NULL,
  offered_external_id text,
  offered_at timestamptz,
  claimed_external_id text,
  claimed_name text,
  claimed_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_link_codes_gateway FOREIGN KEY (gateway_id, kind)
    REFERENCES gateways (id, kind) ON DELETE CASCADE,
  CONSTRAINT gateway_link_codes_kind CHECK (kind = 'telegram'),
  CONSTRAINT gateway_link_codes_one_per_person UNIQUE (gateway_id, user_id),
  CONSTRAINT gateway_link_codes_one_code UNIQUE (gateway_id, code_hash),
  CONSTRAINT gateway_link_codes_hash CHECK (code_hash ~ '^[0-9a-f]{64}$'),
  CONSTRAINT gateway_link_codes_offer CHECK (
    (offered_external_id IS NULL) = (offered_at IS NULL)
    AND (offered_external_id IS NULL OR offered_external_id ~ '^[1-9][0-9]{0,19}$')
  ),
  CONSTRAINT gateway_link_codes_claim CHECK (
    (claimed_external_id IS NULL) = (claimed_at IS NULL)
    AND (claimed_external_id IS NULL) = (claimed_name IS NULL)
    AND (claimed_external_id IS NULL OR claimed_external_id = offered_external_id)
  ),
  CONSTRAINT gateway_link_codes_name CHECK (
    claimed_name IS NULL OR char_length(claimed_name) BETWEEN 1 AND 80
  )
);

CREATE INDEX gateway_link_codes_expiry_idx ON gateway_link_codes (expires_at);
CREATE INDEX gateway_link_codes_offer_idx ON gateway_link_codes (gateway_id, offered_external_id)
  WHERE claimed_at IS NULL;

ALTER TABLE gateway_link_codes ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_link_codes AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON gateway_link_codes TO mimorya_app;

-- Forgets every company's link codes expired before `p_before`; how many.
-- Neither a code's digest nor whom Telegram sent it back from stays.
CREATE FUNCTION system_purge_gateway_link_codes(p_before timestamptz)
RETURNS bigint
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM gateway_link_codes WHERE expires_at < p_before RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_gateway_link_codes(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_gateway_link_codes(timestamptz) TO mimorya_app;
