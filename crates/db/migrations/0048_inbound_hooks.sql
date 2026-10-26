-- Inbound webhooks a person sets for their agent (ADR-109): when another
-- system posts to one, with its secret token, the instruction is sent as
-- theirs, in a new conversation of theirs, with their rights as they stand
-- then, followed by what the system posted, as untrusted data. Only the
-- token's SHA-256 digest is kept. Deliveries are counted per hour (a fixed
-- window: `window_started_at`, `window_deliveries`). What became of the last
-- delivery is kept for them to read, with the conversation and run it
-- started. Never what was posted: that is in the conversation alone.
CREATE TABLE inbound_hooks (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  title text NOT NULL,
  instruction text NOT NULL,
  token_hash text NOT NULL,
  enabled boolean NOT NULL DEFAULT true,
  window_started_at timestamptz,
  window_deliveries integer NOT NULL DEFAULT 0,
  last_received_at timestamptz,
  last_outcome text,
  last_problem text,
  last_conversation_id uuid REFERENCES conversations(id) ON DELETE SET NULL,
  last_run_id uuid REFERENCES agent_runs(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT inbound_hooks_title CHECK (char_length(title) BETWEEN 1 AND 120),
  CONSTRAINT inbound_hooks_instruction CHECK (char_length(instruction) BETWEEN 1 AND 4000),
  CONSTRAINT inbound_hooks_token_hash CHECK (token_hash ~ '^[0-9a-f]{64}$'),
  CONSTRAINT inbound_hooks_window CHECK (window_deliveries >= 0),
  CONSTRAINT inbound_hooks_outcome CHECK (
    last_outcome IN ('started', 'limited', 'refused', 'failed')
  )
);

CREATE INDEX inbound_hooks_user_idx ON inbound_hooks (tenant_id, user_id);

ALTER TABLE inbound_hooks ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON inbound_hooks AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON inbound_hooks TO mimorya_app;

-- The company of the enabled hook `hook_id` whose token digest is
-- `token_hash`, before any session: a delivery names the hook, and proves
-- itself by its token. Nothing else crosses the companies' boundary, and an
-- unknown hook, a wrong token and a paused hook answer alike.
CREATE FUNCTION auth_inbound_hook_tenant(hook_id uuid, token_hash text)
RETURNS uuid
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT inbound_hooks.tenant_id
    FROM inbound_hooks
   WHERE inbound_hooks.id = hook_id
     AND inbound_hooks.token_hash = auth_inbound_hook_tenant.token_hash
     AND inbound_hooks.enabled
$$;

REVOKE ALL ON FUNCTION auth_inbound_hook_tenant(uuid, text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_inbound_hook_tenant(uuid, text) TO mimorya_app;

-- How a run was started: `web` from the application, `hook` when another
-- system posted to its author's hook. A run read again after a pause or a
-- stop keeps it: what it was sent never writes into its author's memory
-- directly, and policies can read it (`context.channel`). A sub-agent's run
-- has its parent's; `evaluation` is never stored (an evaluation opens no run).
ALTER TABLE agent_runs ADD COLUMN channel text NOT NULL DEFAULT 'web'
  CHECK (channel IN ('web', 'hook'));
