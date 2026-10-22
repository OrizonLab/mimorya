-- Who stands in for an absent person on the requests a manager decides
-- (ADR-071): from `starts_at` until `ends_at`, the delegate may decide the
-- pending requests routed to `user_id`, provided they hold
-- `approval:decide` over whom each is for. A delegation grants no right,
-- and one per person: declared again, it is replaced. Ended, it is deleted
-- by the periodic maintenance.
CREATE TABLE approval_delegations (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  delegate_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  starts_at timestamptz NOT NULL,
  ends_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, user_id),
  CONSTRAINT approval_delegations_not_oneself CHECK (delegate_user_id <> user_id),
  CONSTRAINT approval_delegations_period CHECK (ends_at > starts_at)
);

CREATE INDEX approval_delegations_delegate_idx
  ON approval_delegations (tenant_id, delegate_user_id);

ALTER TABLE approval_delegations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON approval_delegations AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON approval_delegations TO mimorya_app;

-- Ended delegations of every company, deleted by the periodic maintenance:
-- one leaves nothing behind but its audit entries.
CREATE FUNCTION system_purge_ended_approval_delegations()
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH deleted AS (
    DELETE FROM approval_delegations WHERE ends_at <= now() RETURNING 1
  )
  SELECT count(*)::integer FROM deleted
$$;

REVOKE ALL ON FUNCTION system_purge_ended_approval_delegations() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_ended_approval_delegations() TO mimorya_app;
