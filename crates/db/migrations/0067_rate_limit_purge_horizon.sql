-- The shared rate limit windows are purged only once long over (ADR-152):
-- whatever horizon the caller gives, never a window opened within the last
-- two hours, twice the longest window (an hour, the reset links'). The
-- application role, which alone calls it, can thereby no longer forget the
-- counts still running, as a horizon of now would.
CREATE OR REPLACE FUNCTION system_purge_rate_limits(p_before timestamptz)
RETURNS bigint
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM rate_limit_windows
     WHERE window_start < least(p_before, now() - interval '2 hours')
    RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_rate_limits(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_rate_limits(timestamptz) TO mimorya_app;
