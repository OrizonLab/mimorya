-- Requests a person asks their agent to make on a schedule (ADR-080): on
-- chosen days of the week, at a time of day in their time zone, the
-- instruction is sent as theirs, in a new conversation of theirs, with
-- their rights as they stand then. `next_run_at` is when it runs next, null
-- while it is paused. What became of the last occurrence is kept for them
-- to read: the conversation and run it started, or why it did not run.
CREATE TABLE scheduled_tasks (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  title text NOT NULL,
  instruction text NOT NULL,
  weekdays smallint[] NOT NULL,
  time_of_day text NOT NULL,
  time_zone text NOT NULL,
  next_run_at timestamptz,
  last_run_at timestamptz,
  last_outcome text,
  last_problem text,
  last_conversation_id uuid REFERENCES conversations(id) ON DELETE SET NULL,
  last_run_id uuid REFERENCES agent_runs(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT scheduled_tasks_title CHECK (char_length(title) BETWEEN 1 AND 120),
  CONSTRAINT scheduled_tasks_instruction CHECK (char_length(instruction) BETWEEN 1 AND 4000),
  CONSTRAINT scheduled_tasks_weekdays CHECK (
    cardinality(weekdays) BETWEEN 1 AND 7 AND weekdays <@ ARRAY[1, 2, 3, 4, 5, 6, 7]::smallint[]
  ),
  CONSTRAINT scheduled_tasks_time CHECK (time_of_day ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'),
  CONSTRAINT scheduled_tasks_outcome CHECK (
    last_outcome IN ('started', 'still_running', 'missed', 'refused', 'failed')
  )
);

CREATE INDEX scheduled_tasks_user_idx ON scheduled_tasks (tenant_id, user_id);
CREATE INDEX scheduled_tasks_due_idx ON scheduled_tasks (next_run_at)
  WHERE next_run_at IS NOT NULL;

ALTER TABLE scheduled_tasks ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON scheduled_tasks AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON scheduled_tasks TO mimorya_app;

-- The tasks of every company due by `due_by`, soonest first, a batch at a
-- time: each is then fired in its own company. Only their company and
-- identifier cross the companies' boundary.
CREATE FUNCTION system_due_scheduled_tasks(due_by timestamptz, batch integer)
RETURNS TABLE (tenant_id uuid, task_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT scheduled_tasks.tenant_id, scheduled_tasks.id
    FROM scheduled_tasks
   WHERE next_run_at <= due_by
   ORDER BY next_run_at
   LIMIT batch
$$;

REVOKE ALL ON FUNCTION system_due_scheduled_tasks(timestamptz, integer) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_due_scheduled_tasks(timestamptz, integer) TO mimorya_app;
