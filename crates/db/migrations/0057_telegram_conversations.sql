-- Conversations with the agent through a company's Telegram bot (ADR-135):
-- a linked person writes to the bot in their private chat with it, and is
-- answered there.

-- A run its author started by writing to the Telegram bot: `telegram`.
-- Policies can read it (`context.channel`), as the others.
ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_channel_check;
ALTER TABLE agent_runs ADD CONSTRAINT agent_runs_channel_check
  CHECK (channel IN ('web', 'hook', 'slack', 'telegram'));

-- Where a conversation is held is of its gateway's service, which the
-- database holds it to. In Slack, a thread of the person's direct
-- conversation with the application, as before. In Telegram, the person's
-- private chat with the bot (named by their identifier, as is the chat),
-- which holds one conversation at a time: the one their message opened
-- (`thread_ts` is that message's number in the chat), followed by their
-- next messages until they ask for a new one (`/nouvelle`), which closes it
-- (`closed_at`). One open conversation per person and chat; a Slack thread
-- is never closed.
ALTER TABLE gateway_threads
  ADD COLUMN kind text,
  ADD COLUMN closed_at timestamptz;

UPDATE gateway_threads t SET kind = g.kind FROM gateways g WHERE g.id = t.gateway_id;

ALTER TABLE gateway_threads
  ALTER COLUMN kind SET NOT NULL,
  DROP CONSTRAINT gateway_threads_external_id,
  DROP CONSTRAINT gateway_threads_channel_id,
  DROP CONSTRAINT gateway_threads_thread_ts,
  ADD CONSTRAINT gateway_threads_gateway_kind FOREIGN KEY (gateway_id, kind)
    REFERENCES gateways (id, kind) ON DELETE CASCADE,
  ADD CONSTRAINT gateway_threads_of_kind CHECK (
    (kind = 'slack' AND external_id ~ '^[UW][A-Z0-9]{2,30}$'
      AND channel_id ~ '^D[A-Z0-9]{2,30}$'
      AND thread_ts ~ '^[0-9]{1,12}\.[0-9]{1,9}$'
      AND closed_at IS NULL)
    OR (kind = 'telegram' AND external_id ~ '^[1-9][0-9]{0,19}$'
      AND channel_id = external_id
      AND thread_ts ~ '^[1-9][0-9]{0,19}$')
  );

CREATE UNIQUE INDEX gateway_threads_one_open_chat
  ON gateway_threads (gateway_id, channel_id, user_id)
  WHERE kind = 'telegram' AND closed_at IS NULL;

GRANT UPDATE (closed_at) ON gateway_threads TO mimorya_app;

-- As before (0053), the answers of a run are written where its
-- conversation is held only when it was started there: its channel is
-- that service's.
CREATE OR REPLACE FUNCTION system_unreplied_gateway_events(
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
                     AND r.channel = t.kind
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
