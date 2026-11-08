-- Retention applied (DATA_MODEL §5, ADR-185): the sessions ended thirty
-- days ago, the episodes kept a year and the memories past their term, the
-- personal memories of an account disabled thirty days ago are deleted.

-- When an account was disabled, its personal memories kept thirty days past
-- it. Kept by the database itself, whatever disables the account (the
-- administration, SCIM, an operator): set when it becomes disabled, cleared
-- when it no longer is. The accounts already disabled count from now.
ALTER TABLE users ADD COLUMN disabled_at timestamptz;

UPDATE users SET disabled_at = now() WHERE status = 'disabled';

CREATE FUNCTION users_disabled_at()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.status = 'disabled' THEN
    IF TG_OP = 'INSERT' OR OLD.status <> 'disabled' THEN
      NEW.disabled_at := now();
    ELSE
      NEW.disabled_at := OLD.disabled_at;
    END IF;
  ELSE
    NEW.disabled_at := NULL;
  END IF;
  RETURN NEW;
END
$$;

-- On every statement that names the status or the date: the date is the
-- database's, never what a statement sets; one that names neither leaves
-- both as they are.
CREATE TRIGGER users_disabled_at
  BEFORE INSERT OR UPDATE OF status, disabled_at ON users
  FOR EACH ROW EXECUTE FUNCTION users_disabled_at();

-- The accounts gone for thirty days, found without reading every account.
CREATE INDEX users_departed ON users (disabled_at) WHERE status = 'disabled';

-- The sessions ended (revoked, idle past their term or past their absolute
-- one) more than thirty days ago, in every company: what they kept of who
-- signed in, from where and with what, goes with them.
CREATE FUNCTION system_purge_ended_sessions()
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH deleted AS (
    DELETE FROM sessions
     WHERE least(coalesce(revoked_at, 'infinity'::timestamptz), expires_at, absolute_expires_at)
           < now() - interval '30 days'
    RETURNING 1
  )
  SELECT count(*)::integer FROM deleted
$$;

REVOKE ALL ON FUNCTION system_purge_ended_sessions() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_ended_sessions() TO mimorya_app;

-- The memories past their retention, in every company, and how many each
-- company lost, and why: `aged`, an episode recorded more than a year ago
-- or a memory past its own term; `departed`, a personal memory of an
-- account disabled more than thirty days ago (one both is `aged`). Each company that lost some
-- finds it in its audit log, in the same statement, by the platform
-- (`memory.purged`, the counts alone): never their content.
CREATE FUNCTION system_purge_retained_memories()
RETURNS TABLE (tenant_id uuid, aged integer, departed integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH deleted AS (
    DELETE FROM memory_items m
     WHERE (m.kind = 'episode' AND m.created_at < now() - interval '12 months')
        OR m.expires_at <= now()
        OR (m.scope = 'user' AND (m.tenant_id, m.owner_user_id) IN (
              SELECT u.tenant_id, u.id FROM users u
               WHERE u.status = 'disabled'
                 AND u.disabled_at < now() - interval '30 days'))
    RETURNING m.tenant_id,
              (m.kind = 'episode' AND m.created_at < now() - interval '12 months')
              OR coalesce(m.expires_at <= now(), false) AS aged
  ),
  lost AS (
    SELECT deleted.tenant_id,
           (count(*) FILTER (WHERE deleted.aged))::integer AS aged,
           (count(*) FILTER (WHERE NOT deleted.aged))::integer AS departed
      FROM deleted
     GROUP BY deleted.tenant_id
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

REVOKE ALL ON FUNCTION system_purge_retained_memories() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_retained_memories() TO mimorya_app;

-- The episodes and the memories with a term, found without reading every
-- memory; a departed account's, through its owner.
CREATE INDEX memory_items_episodes ON memory_items (created_at) WHERE kind = 'episode';

CREATE INDEX memory_items_expiring ON memory_items (expires_at) WHERE expires_at IS NOT NULL;
