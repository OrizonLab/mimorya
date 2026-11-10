-- Each company's own retention durations (DATA_MODEL §5, ADR-195): how
-- long its idle conversations, its episodes and the personal memories of
-- its accounts gone are kept, within the platform's bounds, the platform's
-- own without its choice. A shortening waits seven days before the purge
-- applies it, a lengthening applies at once: one who may set them cannot
-- delete at once what the company keeps. The bounds, the platform's
-- durations and the rule are the database's.

-- The platform's duration of each kind: conversations idle 24 months,
-- episodes 12 months, a departed account's personal memories 30 days.
CREATE FUNCTION retention_platform(p_kind text)
RETURNS integer
LANGUAGE sql
IMMUTABLE
SET search_path = public, pg_temp
AS $$
  SELECT CASE p_kind
           WHEN 'conversations' THEN 24
           WHEN 'episodes' THEN 12
           WHEN 'departed' THEN 30
         END
$$;

-- Whether `p_duration` is one a company may set for the kind `p_kind`:
-- conversations from 1 to 60 months, episodes from 1 to 36 months, a
-- departed account's personal memories from 7 to 90 days.
CREATE FUNCTION retention_allowed(p_kind text, p_duration integer)
RETURNS boolean
LANGUAGE sql
IMMUTABLE
SET search_path = public, pg_temp
AS $$
  SELECT CASE p_kind
           WHEN 'conversations' THEN p_duration BETWEEN 1 AND 60
           WHEN 'episodes' THEN p_duration BETWEEN 1 AND 36
           WHEN 'departed' THEN p_duration BETWEEN 7 AND 90
           ELSE false
         END
$$;

-- What the purge applies of a duration `p_duration` chosen at
-- `p_changed_at` when `p_before` applied: for seven days, the longer of the
-- two; then the one chosen.
CREATE FUNCTION retention_in_effect(p_duration integer, p_before integer, p_changed_at timestamptz)
RETURNS integer
LANGUAGE sql
STABLE
SET search_path = public, pg_temp
AS $$
  SELECT CASE WHEN p_changed_at > now() - interval '7 days'
              THEN greatest(p_duration, p_before)
              ELSE p_duration
         END
$$;

-- A company's duration of one kind, in months for conversations and
-- episodes, in days for a departed account's memories: the one it chose,
-- what applied when it chose it (`before_duration`), and when. Who chose it
-- is forgotten with their account. A kind without a row has the platform's.
CREATE TABLE tenant_retention_durations (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  kind text NOT NULL,
  duration integer NOT NULL,
  before_duration integer NOT NULL,
  changed_at timestamptz NOT NULL DEFAULT now(),
  updated_by_user_id uuid,
  PRIMARY KEY (tenant_id, kind),
  CONSTRAINT tenant_retention_durations_kind
    CHECK (kind IN ('conversations', 'episodes', 'departed')),
  CONSTRAINT tenant_retention_durations_bounds
    CHECK (retention_allowed(kind, duration)),
  -- What applied before is a duration the company had: its own, or the
  -- platform's, which is within the bounds too.
  CONSTRAINT tenant_retention_durations_before
    CHECK (retention_allowed(kind, before_duration)),
  CONSTRAINT tenant_retention_durations_updated_by_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id)
);

CREATE INDEX tenant_retention_durations_updated_by ON tenant_retention_durations
  (tenant_id, updated_by_user_id) WHERE updated_by_user_id IS NOT NULL;

ALTER TABLE tenant_retention_durations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_retention_durations AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- What applied before a change and when it was made are the database's,
-- never what a statement sets: a first change follows the platform's
-- duration; another, what the purge applied until then; one that leaves
-- the duration as it was changes neither. The kind of a row stays.
CREATE FUNCTION tenant_retention_durations_changed()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF TG_OP = 'INSERT' THEN
    NEW.before_duration := retention_platform(NEW.kind);
    NEW.changed_at := now();
  ELSE
    NEW.kind := OLD.kind;
    IF NEW.duration IS DISTINCT FROM OLD.duration THEN
      NEW.before_duration := retention_in_effect(OLD.duration, OLD.before_duration, OLD.changed_at);
      NEW.changed_at := now();
    ELSE
      NEW.before_duration := OLD.before_duration;
      NEW.changed_at := OLD.changed_at;
    END IF;
  END IF;
  RETURN NEW;
END
$$;

CREATE TRIGGER tenant_retention_durations_changed
  BEFORE INSERT OR UPDATE ON tenant_retention_durations
  FOR EACH ROW EXECUTE FUNCTION tenant_retention_durations_changed();

-- A duration is set and changed, never removed: back to the platform's is
-- a change like any other, and waits as one when it shortens. The
-- application writes the duration and who chose it, nothing else.
GRANT SELECT ON tenant_retention_durations TO mimorya_app;
GRANT INSERT (tenant_id, kind, duration, updated_by_user_id) ON tenant_retention_durations
  TO mimorya_app;
GRANT UPDATE (duration, updated_by_user_id) ON tenant_retention_durations TO mimorya_app;

-- What each company's purge applies now, as horizons: before them, its
-- episodes, its departed accounts' memories and its idle conversations go.
CREATE FUNCTION retention_horizons()
RETURNS TABLE (tenant_id uuid, episodes_before timestamptz, departed_before timestamptz,
               conversations_before timestamptz)
LANGUAGE sql
STABLE
SET search_path = public, pg_temp
AS $$
  SELECT t.id,
         now() - make_interval(months => coalesce(
           (SELECT retention_in_effect(r.duration, r.before_duration, r.changed_at)
              FROM tenant_retention_durations r
             WHERE r.tenant_id = t.id AND r.kind = 'episodes'),
           retention_platform('episodes'))),
         now() - make_interval(days => coalesce(
           (SELECT retention_in_effect(r.duration, r.before_duration, r.changed_at)
              FROM tenant_retention_durations r
             WHERE r.tenant_id = t.id AND r.kind = 'departed'),
           retention_platform('departed'))),
         now() - make_interval(months => coalesce(
           (SELECT retention_in_effect(r.duration, r.before_duration, r.changed_at)
              FROM tenant_retention_durations r
             WHERE r.tenant_id = t.id AND r.kind = 'conversations'),
           retention_platform('conversations')))
    FROM tenants t
$$;

-- Read by the purges alone, which run as the owner across the companies.
REVOKE ALL ON FUNCTION retention_horizons() FROM PUBLIC;

-- The memories past their company's retention (ADR-185), its own durations
-- now: `aged`, an episode recorded before its company's horizon or a memory
-- past its own term; `departed`, a personal memory of an account disabled
-- before it. Audited as before (`memory.purged`, the counts alone).
CREATE OR REPLACE FUNCTION system_purge_retained_memories()
RETURNS TABLE (tenant_id uuid, aged integer, departed integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH horizons AS (
    SELECT h.tenant_id, h.episodes_before, h.departed_before FROM retention_horizons() h
  ),
  -- Two deletions of disjoint memories, as before; the latest horizon of
  -- any company bounds what the episodes' index reads.
  aged AS (
    DELETE FROM memory_items m
     USING horizons h
     WHERE m.tenant_id = h.tenant_id
       AND ((m.kind = 'episode'
             AND m.created_at < h.episodes_before
             AND m.created_at < (SELECT max(episodes_before) FROM horizons))
            OR m.expires_at <= now())
    RETURNING m.tenant_id
  ),
  departed AS (
    DELETE FROM memory_items m
     USING users u, horizons h
     WHERE u.status = 'disabled'
       AND u.disabled_at < h.departed_before
       AND u.disabled_at < (SELECT max(departed_before) FROM horizons)
       AND h.tenant_id = u.tenant_id
       AND m.tenant_id = u.tenant_id
       AND m.owner_user_id = u.id
       AND m.scope = 'user'
       AND NOT (m.kind = 'episode' AND m.created_at < h.episodes_before)
       AND NOT coalesce(m.expires_at <= now(), false)
    RETURNING m.tenant_id
  ),
  lost AS (
    SELECT counted.tenant_id,
           (count(*) FILTER (WHERE counted.aged))::integer AS aged,
           (count(*) FILTER (WHERE NOT counted.aged))::integer AS departed
      FROM (SELECT aged.tenant_id, true AS aged FROM aged
            UNION ALL
            SELECT departed.tenant_id, false FROM departed) AS counted
     GROUP BY counted.tenant_id
  ),
  audited AS (
    INSERT INTO audit_log (tenant_id, actor_type, action, outcome, metadata)
    SELECT lost.tenant_id, 'system', 'memory.purged', 'success',
           jsonb_build_object('aged', lost.aged, 'departed', lost.departed)
      FROM lost
    RETURNING 1
  )
  SELECT lost.tenant_id, lost.aged, lost.departed FROM lost
$$;

-- The conversations idle past their company's retention (ADR-186), its own
-- duration now, as before otherwise: the oldest first, `p_batch` at most,
-- one a run still lives in left, audited (`conversation.purged`).
CREATE OR REPLACE FUNCTION system_purge_idle_conversations(p_batch integer)
RETURNS TABLE (tenant_id uuid, conversations integer, messages integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH horizons AS (
    SELECT h.tenant_id, h.conversations_before FROM retention_horizons() h
  ),
  -- Locked as they are chosen: a conversation a turn holds is skipped; one
  -- a turn moved meanwhile is read again, found recent and left. The latest
  -- horizon of any company bounds what the index reads.
  chosen AS (
    SELECT c.tenant_id, c.id
      FROM conversations c
      JOIN horizons h ON h.tenant_id = c.tenant_id
     WHERE c.updated_at < h.conversations_before
       AND c.updated_at < (SELECT max(conversations_before) FROM horizons)
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

-- The platform's administrator role of the companies already there gains
-- what the new ones are created with: setting the company's retention.
UPDATE roles SET permissions = permissions || ARRAY['retention:manage']
  WHERE is_system AND key = 'tenant_admin'
    AND NOT 'retention:manage' = ANY (permissions);
