-- Conversations with the agent through a company's Slack application
-- (ADR-127): a linked person writes to it, and is answered in the thread.

-- A run its author started by writing to the Slack application: `slack`.
-- Policies can read it (`context.channel`), as the others.
ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_channel_check;
ALTER TABLE agent_runs ADD CONSTRAINT agent_runs_channel_check
  CHECK (channel IN ('web', 'hook', 'slack'));

-- The thread of a person's direct conversation with the application that a
-- conversation of theirs is held in: opened by their message outside any
-- thread (the thread is named after it), followed by their replies in it.
-- The Slack user who wrote is kept: the answers go to the thread only while
-- that Slack user is still the person's linked identity. `active_at` is its
-- latest turn or answer: the answers pass reads the threads active lately.
-- A thread goes with its gateway, its person and its conversation.
CREATE TABLE gateway_threads (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  gateway_id uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  external_id text NOT NULL,
  channel_id text NOT NULL,
  thread_ts text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  active_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_threads_one_per_conversation UNIQUE (conversation_id),
  CONSTRAINT gateway_threads_one_per_thread UNIQUE (gateway_id, channel_id, thread_ts),
  CONSTRAINT gateway_threads_external_id CHECK (external_id ~ '^[UW][A-Z0-9]{2,30}$'),
  CONSTRAINT gateway_threads_channel_id CHECK (channel_id ~ '^D[A-Z0-9]{2,30}$'),
  CONSTRAINT gateway_threads_thread_ts CHECK (thread_ts ~ '^[0-9]{1,12}\.[0-9]{1,9}$')
);

CREATE INDEX gateway_threads_user_idx ON gateway_threads (tenant_id, user_id);
CREATE INDEX gateway_threads_active_idx ON gateway_threads (active_at);

ALTER TABLE gateway_threads ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_threads AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE (active_at) ON gateway_threads TO mimorya_app;

-- What became of the answer to a run's end or pause (its `run_finished`
-- event), in the thread its conversation is held in: `sent`; `refused` by
-- Slack; `failed` after its attempts; `skipped` (gateway paused, identity
-- no longer linked). Without an outcome, it is tried again after
-- `retry_after`. Only identifiers, never what was written: that is in the
-- conversation.
CREATE TABLE gateway_replies (
  event_id bigint PRIMARY KEY REFERENCES run_events(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  thread_id uuid NOT NULL REFERENCES gateway_threads(id) ON DELETE CASCADE,
  outcome text,
  attempts integer NOT NULL DEFAULT 1,
  retry_after timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_replies_outcome CHECK (outcome IN ('sent', 'refused', 'failed', 'skipped')),
  CONSTRAINT gateway_replies_pending CHECK ((outcome IS NULL) = (retry_after IS NOT NULL))
);

CREATE INDEX gateway_replies_thread_idx ON gateway_replies (thread_id);

ALTER TABLE gateway_replies ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_replies AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE ON gateway_replies TO mimorya_app;

-- The ends and pauses since `p_since`, of every company, of runs started
-- from a gateway in a conversation held in a thread active since
-- `p_active_since`, still to answer there at `p_now`: never answered, not
-- waiting to be tried again, and no earlier answer of the same thread
-- waiting (a thread's answers are written in order) while it can still be
-- tried, its end or pause since `p_since` too. Their company and
-- their event, oldest first, at most `p_limit`; the run and the event of
-- the thread's own company. Read from the threads, so that its cost
-- follows the conversations held in Slack, not every run. Nothing else
-- crosses the companies' boundary: what is answered is read in each company.
CREATE FUNCTION system_unreplied_gateway_events(
  p_active_since timestamptz, p_since timestamptz, p_now timestamptz, p_limit integer)
RETURNS TABLE (tenant_id uuid, event_id bigint)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT e.tenant_id, e.id
    FROM gateway_threads t
    JOIN agent_runs r ON r.conversation_id = t.conversation_id
                     AND r.tenant_id = t.tenant_id
                     AND r.channel = 'slack'
    JOIN run_events e ON e.run_id = r.id
                     AND e.tenant_id = t.tenant_id
                     AND e.type = 'run_finished'
   WHERE t.active_at > p_active_since
     AND e.created_at > p_since
     AND NOT EXISTS (
       SELECT 1 FROM gateway_replies g
        WHERE g.event_id = e.id
          AND (g.outcome IS NOT NULL OR g.retry_after > p_now))
     AND NOT EXISTS (
       SELECT 1 FROM gateway_replies w
         JOIN run_events we ON we.id = w.event_id
        WHERE w.thread_id = t.id AND w.outcome IS NULL AND w.event_id < e.id
          AND we.created_at > p_since)
   ORDER BY e.id
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_unreplied_gateway_events(timestamptz, timestamptz, timestamptz, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_unreplied_gateway_events(timestamptz, timestamptz, timestamptz, integer) TO mimorya_app;
