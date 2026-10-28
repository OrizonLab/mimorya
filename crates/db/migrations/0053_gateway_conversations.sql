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
-- that Slack user is still the person's linked identity. A thread goes
-- with its gateway, its person and its conversation.
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
  CONSTRAINT gateway_threads_one_per_conversation UNIQUE (conversation_id),
  CONSTRAINT gateway_threads_one_per_thread UNIQUE (gateway_id, channel_id, thread_ts),
  CONSTRAINT gateway_threads_external_id CHECK (external_id ~ '^[UW][A-Z0-9]{2,30}$'),
  CONSTRAINT gateway_threads_channel_id CHECK (channel_id ~ '^D[A-Z0-9]{2,30}$'),
  CONSTRAINT gateway_threads_thread_ts CHECK (thread_ts ~ '^[0-9]{1,12}\.[0-9]{1,9}$')
);

CREATE INDEX gateway_threads_user_idx ON gateway_threads (tenant_id, user_id);

ALTER TABLE gateway_threads ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_threads AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, DELETE ON gateway_threads TO mimorya_app;

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

-- The runs' ends and pauses, by time: the replies pass reads the latest.
CREATE INDEX run_events_finished_idx ON run_events (created_at) WHERE type = 'run_finished';

-- The ends and pauses since `p_since`, of every company, of runs started
-- from a gateway in a conversation held in a thread, still to answer there
-- at `p_now` (never answered, nor waiting to be tried again): their company
-- and their event, oldest first, at most `p_limit`; the run and the thread
-- of the event's own company. Nothing else crosses the companies' boundary:
-- what is answered is read in each company.
CREATE FUNCTION system_unreplied_gateway_events(p_since timestamptz, p_now timestamptz, p_limit integer)
RETURNS TABLE (tenant_id uuid, event_id bigint)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT e.tenant_id, e.id
    FROM run_events e
    JOIN agent_runs r ON r.id = e.run_id AND r.tenant_id = e.tenant_id
    JOIN gateway_threads t ON t.conversation_id = r.conversation_id
                          AND t.tenant_id = e.tenant_id
   WHERE e.type = 'run_finished'
     AND e.created_at > p_since
     AND r.channel = 'slack'
     AND NOT EXISTS (
       SELECT 1 FROM gateway_replies g
        WHERE g.event_id = e.id
          AND (g.outcome IS NOT NULL OR g.retry_after > p_now))
   ORDER BY e.id
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_unreplied_gateway_events(timestamptz, timestamptz, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_unreplied_gateway_events(timestamptz, timestamptz, integer) TO mimorya_app;
