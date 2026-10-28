-- Conversation gateways a company declares (ADR-125): its Slack
-- application, through which its people talk with the agent in Slack, each
-- as themselves. One per service and company. What Slack says of the
-- application when it is declared is kept: its workspace (every event
-- must come from it) and its bot user (whose own messages are never taken
-- for a person's). Its bot token and signing secret are sealed in the
-- secret store, never here.
CREATE TABLE gateways (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  kind text NOT NULL,
  name text NOT NULL,
  enabled boolean NOT NULL DEFAULT true,
  workspace_id text NOT NULL,
  workspace_name text NOT NULL,
  bot_user_id text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateways_kind CHECK (kind IN ('slack')),
  CONSTRAINT gateways_one_per_kind UNIQUE (tenant_id, kind),
  CONSTRAINT gateways_name CHECK (char_length(name) BETWEEN 2 AND 80),
  CONSTRAINT gateways_workspace_id CHECK (workspace_id ~ '^[TE][A-Z0-9]{2,30}$'),
  CONSTRAINT gateways_workspace_name CHECK (char_length(workspace_name) BETWEEN 1 AND 200),
  CONSTRAINT gateways_bot_user_id CHECK (bot_user_id ~ '^[UW][A-Z0-9]{2,30}$')
);

ALTER TABLE gateways ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateways AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON gateways TO mimorya_app;

-- Who a person is on a gateway's service, linked by them from their own
-- session (ADR-125): their Slack user, whose address Slack gave as theirs
-- when they linked it. One per person and gateway; a Slack user is one
-- person's. A link goes with its gateway and its person.
CREATE TABLE gateway_identities (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  gateway_id uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  external_id text NOT NULL,
  address text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_identities_one_per_person UNIQUE (gateway_id, user_id),
  CONSTRAINT gateway_identities_one_person UNIQUE (gateway_id, external_id),
  CONSTRAINT gateway_identities_external_id CHECK (external_id ~ '^[UW][A-Z0-9]{2,30}$'),
  CONSTRAINT gateway_identities_address CHECK (char_length(address) BETWEEN 3 AND 320)
);

CREATE INDEX gateway_identities_user_idx ON gateway_identities (tenant_id, user_id);

ALTER TABLE gateway_identities ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_identities AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON gateway_identities TO mimorya_app;

-- The events a gateway took, by the identifier its service gives them:
-- Slack sends an event again when it was not answered in time, and a retry
-- is taken once. Kept a day, then purged by the maintenance pass.
CREATE TABLE gateway_events (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  gateway_id uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
  event_id text NOT NULL,
  received_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (gateway_id, event_id),
  CONSTRAINT gateway_events_event_id CHECK (char_length(event_id) BETWEEN 1 AND 64)
);

CREATE INDEX gateway_events_received_idx ON gateway_events (received_at);

ALTER TABLE gateway_events ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_events AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, DELETE ON gateway_events TO mimorya_app;

-- The company of the gateway `gateway_id` of `kind`, before any session: a
-- request from the service names its gateway, then proves itself by its
-- signature, checked with the secret read in that company. Nothing else
-- crosses the companies' boundary; a paused gateway is found alike (its
-- requests are answered, nothing is done).
CREATE FUNCTION auth_gateway_tenant(gateway_id uuid, kind text)
RETURNS uuid
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT gateways.tenant_id
    FROM gateways
   WHERE gateways.id = gateway_id
     AND gateways.kind = auth_gateway_tenant.kind
$$;

REVOKE ALL ON FUNCTION auth_gateway_tenant(uuid, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION auth_gateway_tenant(uuid, text) TO mimorya_app;

-- Forgets the events of every company taken before `p_before`; how many.
-- Only identifiers and times are kept, never what an event said.
CREATE FUNCTION system_purge_gateway_events(p_before timestamptz)
RETURNS bigint
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM gateway_events WHERE received_at < p_before RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_gateway_events(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_gateway_events(timestamptz) TO mimorya_app;
