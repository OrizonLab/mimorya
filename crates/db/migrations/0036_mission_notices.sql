-- Notices to a mission's author (ADR-091). A stage paused on requests its
-- author decides is told to them by e-mail, as a scheduled task's run is
-- (ADR-082): it runs while nobody watches. A stop of the service between
-- that pause and the e-mail loses the notice: the periodic catch-up tells
-- the author, in their own company, of those still pending in the paused
-- stage of their mission under way, in an active company, not expired by
-- `p_now`. Only the company and the run cross the companies' boundary.
CREATE FUNCTION system_unnotified_mission_runs(p_now timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
    JOIN agent_runs ON agent_runs.id = approvals.run_id
    JOIN missions ON missions.id = agent_runs.mission_id
     AND missions.owner_user_id = approvals.requested_for_user_id
     AND missions.status = 'in_progress'
    JOIN tenants ON tenants.id = approvals.tenant_id
   WHERE approvals.status = 'pending'
     AND approvals.routing = 'requester'
     AND (approvals.expires_at IS NULL OR approvals.expires_at > p_now)
     AND agent_runs.status = 'awaiting_approval'
     AND tenants.status = 'active'
     AND NOT EXISTS (
           SELECT 1 FROM audit_log
            WHERE audit_log.run_id = approvals.run_id
              AND audit_log.action = 'approval.notified'
              AND audit_log.resource_id = approvals.id::text)
$$;

REVOKE ALL ON FUNCTION system_unnotified_mission_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unnotified_mission_runs(timestamptz) TO mimorya_app;

-- A mission over is told to its author once, by the periodic pass: done,
-- or stopped unfinished (past its limits, its last stage failed, whichever
-- path failed it), not when its author stopped it. `end_notified_at` says
-- when it was told, or tried to be.
ALTER TABLE missions ADD COLUMN end_notified_at timestamptz;

-- The missions not told of their end yet, but those stopped by their
-- author: those over, and those under way whose last stage may have failed.
CREATE INDEX missions_end_unnotified_idx ON missions (status, finished_at)
  WHERE end_notified_at IS NULL AND status <> 'cancelled';

-- Every company's missions over since `p_since`, never told to their
-- author, the company and the author's account active: each is then told
-- of in its own company. A mission under way whose last stage failed or
-- was cancelled is over (ADR-089), since that stage ended.
CREATE FUNCTION system_unnotified_mission_ends(p_since timestamptz)
RETURNS TABLE (tenant_id uuid, mission_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT missions.tenant_id, missions.id
    FROM missions
    JOIN tenants ON tenants.id = missions.tenant_id
    JOIN users ON users.id = missions.owner_user_id
    LEFT JOIN LATERAL (
      SELECT agent_runs.status, agent_runs.finished_at
        FROM agent_runs
       WHERE agent_runs.mission_id = missions.id
       ORDER BY agent_runs.created_at DESC, agent_runs.id DESC
       LIMIT 1) stage ON true
   WHERE missions.end_notified_at IS NULL
     AND missions.status <> 'cancelled'
     AND tenants.status = 'active'
     AND users.status = 'active'
     AND ((missions.status IN ('done', 'failed') AND missions.finished_at > p_since)
       OR (missions.status = 'in_progress'
           AND stage.status IN ('failed', 'cancelled')
           AND stage.finished_at > p_since))
$$;

REVOKE ALL ON FUNCTION system_unnotified_mission_ends(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unnotified_mission_ends(timestamptz) TO mimorya_app;
