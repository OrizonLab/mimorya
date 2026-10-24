-- Shared rate limits (ADR-088): the windows of the authentication limits
-- (sign-ins per client address, failed sign-ins per account, reset links per
-- address, addresses asked about and sign-ins through an identity
-- provider), counted in the database so that every `api` process counts
-- alike. No company's data: a digest of each key and value (client
-- address, e-mail address), kept for its window only, then purged by the
-- maintenance. Unlogged: a crash of the database empties them, which only
-- gives the clients a fresh window.
CREATE UNLOGGED TABLE rate_limit_windows (
  limiter text NOT NULL,
  key_digest bytea NOT NULL,
  window_start timestamptz NOT NULL,
  hits integer NOT NULL,
  PRIMARY KEY (limiter, key_digest)
);

-- The distinct values a key asked about within its window: an address asked
-- about again costs nothing (ADR-063).
CREATE UNLOGGED TABLE rate_limit_values (
  limiter text NOT NULL,
  key_digest bytea NOT NULL,
  value_digest bytea NOT NULL,
  PRIMARY KEY (limiter, key_digest, value_digest),
  FOREIGN KEY (limiter, key_digest) REFERENCES rate_limit_windows ON DELETE CASCADE
);

-- Nobody reads or writes them but the functions below: the application role
-- is granted nothing on them, and row-level security denies every row.
ALTER TABLE rate_limit_windows ENABLE ROW LEVEL SECURITY;
ALTER TABLE rate_limit_values ENABLE ROW LEVEL SECURITY;
CREATE POLICY no_direct_access ON rate_limit_windows USING (false) WITH CHECK (false);
CREATE POLICY no_direct_access ON rate_limit_values USING (false) WITH CHECK (false);

-- Counts one hit of `p_key` for `p_limiter` in its window of `p_seconds`,
-- opening a new window once the last one ended; whether it is allowed (at
-- most `p_limit` a window). A refused hit is not counted on: the count stops
-- one past the limit. Hits on one key are counted one at a time (its row's
-- lock), on the database's clock: each call is a transaction of its own,
-- whose time (`now()`) every comparison reads alike.
CREATE FUNCTION system_rate_limit_hit(
  p_limiter text, p_key bytea, p_limit integer, p_seconds double precision
)
RETURNS boolean
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  INSERT INTO rate_limit_windows AS w (limiter, key_digest, window_start, hits)
  VALUES (p_limiter, p_key, now(), 1)
  ON CONFLICT (limiter, key_digest) DO UPDATE SET
    window_start = CASE
      WHEN w.window_start <= now() - make_interval(secs => p_seconds)
        THEN now()
      ELSE w.window_start END,
    hits = CASE
      WHEN w.window_start <= now() - make_interval(secs => p_seconds) THEN 1
      ELSE least(w.hits + 1, p_limit + 1) END
  RETURNING hits <= p_limit
$$;

REVOKE ALL ON FUNCTION system_rate_limit_hit(text, bytea, integer, double precision) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_rate_limit_hit(text, bytea, integer, double precision) TO mimorya_app;

-- Gives back a hit `system_rate_limit_hit` allowed (a sign-in that
-- succeeded counts for nothing), within the same window only.
CREATE FUNCTION system_rate_limit_release(
  p_limiter text, p_key bytea, p_limit integer, p_seconds double precision
)
RETURNS void
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  UPDATE rate_limit_windows
     SET hits = greatest(least(hits, p_limit) - 1, 0)
   WHERE limiter = p_limiter
     AND key_digest = p_key
     AND window_start > now() - make_interval(secs => p_seconds)
$$;

REVOKE ALL ON FUNCTION system_rate_limit_release(text, bytea, integer, double precision) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_rate_limit_release(text, bytea, integer, double precision) TO mimorya_app;

-- Counts `p_value` for `p_key`, once a window of `p_seconds`; whether it is
-- allowed: one already counted is, a new one only below `p_limit` distinct
-- values. The key's window row is locked first, so that its values are
-- counted one at a time.
CREATE FUNCTION system_rate_limit_distinct(
  p_limiter text, p_key bytea, p_value bytea, p_limit integer, p_seconds double precision
)
RETURNS boolean
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
  v_start timestamptz;
BEGIN
  INSERT INTO rate_limit_windows AS w (limiter, key_digest, window_start, hits)
  VALUES (p_limiter, p_key, now(), 0)
  ON CONFLICT (limiter, key_digest) DO UPDATE SET window_start = w.window_start
  RETURNING window_start INTO v_start;
  IF v_start <= now() - make_interval(secs => p_seconds) THEN
    UPDATE rate_limit_windows SET window_start = now(), hits = 0
     WHERE limiter = p_limiter AND key_digest = p_key;
    DELETE FROM rate_limit_values WHERE limiter = p_limiter AND key_digest = p_key;
  END IF;
  IF EXISTS (SELECT 1 FROM rate_limit_values
              WHERE limiter = p_limiter AND key_digest = p_key AND value_digest = p_value) THEN
    RETURN true;
  END IF;
  IF (SELECT count(*) FROM rate_limit_values
       WHERE limiter = p_limiter AND key_digest = p_key) >= p_limit THEN
    RETURN false;
  END IF;
  INSERT INTO rate_limit_values (limiter, key_digest, value_digest)
  VALUES (p_limiter, p_key, p_value);
  RETURN true;
END
$$;

REVOKE ALL ON FUNCTION system_rate_limit_distinct(text, bytea, bytea, integer, double precision) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_rate_limit_distinct(text, bytea, bytea, integer, double precision) TO mimorya_app;

-- Forgets the windows opened before `p_before`, with their values; how many.
CREATE FUNCTION system_purge_rate_limits(p_before timestamptz)
RETURNS bigint
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM rate_limit_windows WHERE window_start < p_before RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_rate_limits(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_rate_limits(timestamptz) TO mimorya_app;
