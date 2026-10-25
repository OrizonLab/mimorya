-- The skills' administrators told by e-mail of the agent's proposals
-- (ADR-102): every minute, the proposals awaiting a decision and never
-- announced are told, in one e-mail to each administrator of their
-- company. `announced_at` says when, or that the relay refused it; a
-- proposal decided before the pass is not announced.
ALTER TABLE skill_versions ADD COLUMN announced_at timestamptz;

CREATE INDEX skill_versions_unannounced_idx ON skill_versions (tenant_id)
  WHERE proposed AND status = 'candidate' AND announced_at IS NULL;

-- The active companies holding proposals awaiting a decision and never
-- announced: each is then read in its own company.
CREATE FUNCTION system_unannounced_skill_proposals()
RETURNS TABLE (tenant_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT skill_versions.tenant_id
    FROM skill_versions
    JOIN tenants ON tenants.id = skill_versions.tenant_id
   WHERE skill_versions.proposed
     AND skill_versions.status = 'candidate'
     AND skill_versions.announced_at IS NULL
     AND tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION system_unannounced_skill_proposals() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unannounced_skill_proposals() TO mimorya_app;
