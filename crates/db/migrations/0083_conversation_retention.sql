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

-- The conversations idle for twenty-four months, in every company, and
-- what each company lost: how many conversations and messages. A
-- conversation's last activity is its `updated_at`, moved by each message,
-- renaming or archiving; one with a run queued, running or awaiting a
-- validation stays. Each company that lost some finds it in its audit log,
-- in the same statement, by the platform (`conversation.purged`, the
-- counts alone): never what was said.
CREATE FUNCTION system_purge_idle_conversations()
RETURNS TABLE (tenant_id uuid, conversations integer, messages integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  -- Its conditions on the deleted row itself: a message written meanwhile
  -- moves `updated_at`, and the deletion, waiting on it, skips the row.
  WITH deleted AS (
    DELETE FROM conversations c
     WHERE c.updated_at < now() - interval '24 months'
       AND NOT EXISTS (
         SELECT 1 FROM agent_runs r
          WHERE r.tenant_id = c.tenant_id
            AND r.conversation_id = c.id
            AND r.status IN ('queued', 'running', 'awaiting_approval'))
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

REVOKE ALL ON FUNCTION system_purge_idle_conversations() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_idle_conversations() TO mimorya_app;

-- The idle conversations, found without reading every conversation.
CREATE INDEX conversations_idle ON conversations (updated_at);
