-- Conversations kept twenty-four months past their last activity
-- (DATA_MODEL §5, ADR-186): then they go whole, with their messages, runs,
-- run events, tool calls, validations, missions and summaries, unless one
-- of their runs still lives.

-- An audit entry keeps the run it names. The log is kept five years and
-- sealed (ADR-178); a run goes with its conversation. Through a foreign key,
-- a run's deletion would set the entry's `run_id` to null and break its
-- seal. The key is replaced by the check it made when the entry was
-- written: the run named exists, in the entry's company. The run may go
-- later; the entry, unchanged, names the run that was.
ALTER TABLE audit_log DROP CONSTRAINT audit_log_run_id_agent_runs_id_fk;

CREATE FUNCTION audit_log_run_known()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.run_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM agent_runs r WHERE r.tenant_id = NEW.tenant_id AND r.id = NEW.run_id
  ) THEN
    RAISE EXCEPTION 'audit entry names no run of its company'
      USING ERRCODE = 'foreign_key_violation', CONSTRAINT = 'audit_log_run_known';
  END IF;
  RETURN NEW;
END
$$;

-- Run as whoever writes the entry: the application, under row-level
-- security, sees its company's runs alone; the owner sees every run and
-- compares the company.
CREATE TRIGGER audit_log_run_known
  BEFORE INSERT OR UPDATE OF tenant_id, run_id ON audit_log
  FOR EACH ROW EXECUTE FUNCTION audit_log_run_known();

-- The conversations idle for twenty-four months, in every company, the
-- oldest first, `p_batch` at most (from 1 to 500: a backlog goes over several
-- short statements), and what each company lost: how many conversations and
-- messages. A conversation's last activity is its `updated_at`, moved by
-- each message, renaming or archiving; one with a run queued, running or
-- awaiting a validation stays. Each company that lost some finds it in its
-- audit log, in the same statement, by the platform (`conversation.purged`,
-- the counts alone): never what was said.
CREATE FUNCTION system_purge_idle_conversations(p_batch integer)
RETURNS TABLE (tenant_id uuid, conversations integer, messages integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  -- Locked as they are chosen: a conversation a turn holds is skipped; one
  -- a turn moved meanwhile is read again, found recent and left.
  WITH chosen AS (
    SELECT c.tenant_id, c.id
      FROM conversations c
     WHERE c.updated_at < now() - interval '24 months'
       AND NOT EXISTS (
         SELECT 1 FROM agent_runs r
          WHERE r.tenant_id = c.tenant_id
            AND r.conversation_id = c.id
            AND r.status IN ('queued', 'running', 'awaiting_approval'))
     ORDER BY c.updated_at
     LIMIT least(greatest(p_batch, 1), 500)
     FOR UPDATE OF c SKIP LOCKED
  ),
  deleted AS (
    DELETE FROM conversations c
     USING chosen
     WHERE c.tenant_id = chosen.tenant_id AND c.id = chosen.id
    RETURNING c.tenant_id, c.id
  ),
  -- The messages as they were before the statement: gone with it.
  lost AS (
    SELECT d.tenant_id,
           count(*)::integer AS conversations,
           coalesce(sum((SELECT count(*) FROM messages m
                          WHERE m.tenant_id = d.tenant_id AND m.conversation_id = d.id)), 0)::integer
             AS messages
      FROM deleted d
     GROUP BY d.tenant_id
  ),
  audited AS (
    INSERT INTO audit_log (tenant_id, actor_type, action, outcome, metadata)
    SELECT lost.tenant_id, 'system', 'conversation.purged', 'success',
           jsonb_build_object('conversations', lost.conversations, 'messages', lost.messages)
      FROM lost
    RETURNING 1
  )
  SELECT lost.tenant_id, lost.conversations, lost.messages FROM lost
$$;

REVOKE ALL ON FUNCTION system_purge_idle_conversations(integer) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_idle_conversations(integer) TO mimorya_app;

-- The idle conversations, found without reading every conversation.
CREATE INDEX conversations_idle ON conversations (updated_at);

-- What a deleted conversation's rows are named by, found without reading
-- the company's: each key that deletes or empties its rows with them
-- (`crates/db/tests/purge_indexes.rs` fails on one without its index).
CREATE INDEX agent_runs_request_message ON agent_runs (tenant_id, request_message_id)
  WHERE request_message_id IS NOT NULL;
CREATE INDEX messages_run ON messages (tenant_id, run_id) WHERE run_id IS NOT NULL;
CREATE INDEX conversation_summaries_through ON conversation_summaries (tenant_id, through_message_id);
CREATE INDEX tool_invocations_approval ON tool_invocations (tenant_id, approval_id)
  WHERE approval_id IS NOT NULL;
CREATE INDEX gateway_cards_thread ON gateway_cards (tenant_id, thread_id)
  WHERE thread_id IS NOT NULL;
CREATE INDEX usage_records_run ON usage_records (tenant_id, run_id) WHERE run_id IS NOT NULL;
CREATE INDEX memory_items_created_by_run ON memory_items (tenant_id, created_by_run_id)
  WHERE created_by_run_id IS NOT NULL;
CREATE INDEX skill_versions_created_by_run ON skill_versions (tenant_id, created_by_run_id)
  WHERE created_by_run_id IS NOT NULL;
CREATE INDEX deliverables_run ON deliverables (tenant_id, run_id) WHERE run_id IS NOT NULL;
CREATE INDEX scheduled_tasks_last_run ON scheduled_tasks (tenant_id, last_run_id)
  WHERE last_run_id IS NOT NULL;
CREATE INDEX scheduled_tasks_last_conversation ON scheduled_tasks (tenant_id, last_conversation_id)
  WHERE last_conversation_id IS NOT NULL;
CREATE INDEX inbound_hooks_last_run ON inbound_hooks (tenant_id, last_run_id)
  WHERE last_run_id IS NOT NULL;
CREATE INDEX inbound_hooks_last_conversation ON inbound_hooks (tenant_id, last_conversation_id)
  WHERE last_conversation_id IS NOT NULL;
