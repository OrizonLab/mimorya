-- Evaluations of a skill's versions (ADR-104): an administrator asks for
-- one version to be tried on the skill's cases (ADR-103), and for the
-- active one beside it when another is evaluated. The cases are copied in
-- as they stood when it was asked for. A process that executes runs plays
-- them, one evaluation at a time across the processes, under the lock of
-- its sweep: an evaluation found running by the process holding that lock
-- is one a stopped process left. One evaluation queued or running per
-- company. The last five of a skill are kept, ninety days at most after
-- their end; they go with their skill. An answer and the judge's issues
-- are read by whoever asked for the evaluation, with whose rights it ran;
-- its author leaving, nobody reads them any more, and they go with the
-- evaluation.
CREATE TABLE skill_evaluations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  skill_id uuid NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  version integer NOT NULL CHECK (version >= 1),
  -- The active version tried beside it, when another one is evaluated.
  reference_version integer CHECK (reference_version >= 1),
  launched_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  status text NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued', 'running', 'completed', 'failed')),
  failure text,
  -- [{ "name", "request", "criteria" }], in the order of the cases.
  cases jsonb NOT NULL CHECK (jsonb_typeof(cases) = 'array'),
  input_tokens bigint NOT NULL DEFAULT 0,
  output_tokens bigint NOT NULL DEFAULT 0,
  cost_micros bigint NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  started_at timestamptz,
  finished_at timestamptz,
  CONSTRAINT skill_evaluations_other_reference CHECK (reference_version <> version)
);

CREATE INDEX skill_evaluations_skill_idx ON skill_evaluations (skill_id, created_at DESC);
CREATE UNIQUE INDEX skill_evaluations_one_per_company
  ON skill_evaluations (tenant_id) WHERE status IN ('queued', 'running');

-- One case played for one version: the evaluated one, or the reference.
CREATE TABLE skill_evaluation_results (
  evaluation_id uuid NOT NULL REFERENCES skill_evaluations(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  position integer NOT NULL CHECK (position >= 0),
  reference boolean NOT NULL,
  outcome text NOT NULL CHECK (outcome IN ('passed', 'failed', 'error')),
  issues text[] NOT NULL DEFAULT '{}',
  answer text,
  error text,
  skill_loaded boolean NOT NULL,
  -- [{ "name", "summary", "isError" }], in the order they were called.
  tools jsonb NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(tools) = 'array'),
  cost_micros bigint NOT NULL DEFAULT 0,
  finished_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (evaluation_id, position, reference)
);

ALTER TABLE skill_evaluations ENABLE ROW LEVEL SECURITY;
ALTER TABLE skill_evaluation_results ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON skill_evaluations AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY tenant_isolation ON skill_evaluation_results AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON skill_evaluations TO mimorya_app;
-- A result is written once.
GRANT SELECT, INSERT, DELETE ON skill_evaluation_results TO mimorya_app;

-- The evaluations a process that executes runs has to play: those queued
-- in an active company, oldest first, and those left running by a stopped
-- process, wherever. The company and the evaluation only cross the
-- companies' boundary; each is read and played in its own company.
CREATE FUNCTION system_pending_skill_evaluations()
RETURNS TABLE (tenant_id uuid, id uuid, running boolean)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT e.tenant_id, e.id, e.status = 'running'
    FROM skill_evaluations e
    JOIN tenants t ON t.id = e.tenant_id
   WHERE e.status = 'running'
      OR (e.status = 'queued' AND t.status = 'active')
   ORDER BY e.created_at, e.id
$$;

REVOKE ALL ON FUNCTION system_pending_skill_evaluations() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_pending_skill_evaluations() TO mimorya_app;

-- Deletes the evaluations of every company that ended before `p_before`,
-- results with them; how many.
CREATE FUNCTION system_purge_skill_evaluations(p_before timestamptz)
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM skill_evaluations
     WHERE status IN ('completed', 'failed') AND finished_at < p_before
    RETURNING 1
  )
  SELECT count(*)::integer FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_skill_evaluations(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_skill_evaluations(timestamptz) TO mimorya_app;
