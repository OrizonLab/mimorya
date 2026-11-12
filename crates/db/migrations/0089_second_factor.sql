-- The second factor of the accounts that sign in with a password (ADR-200).
--
-- An account's authenticator secret, sealed under the server's master key
-- (AES-256-GCM, for this account alone), with the last 30-second step a
-- code was accepted for: no code passes twice, nor one older. A row without
-- `enabled_at` is an enrolment that waits for its first code. The codes
-- refused in a row since the last one accepted: past a hundred, only a
-- recovery code is (NIST SP 800-63B).
CREATE TABLE second_factors (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL,
  secret_ciphertext bytea NOT NULL,
  secret_iv bytea NOT NULL,
  secret_tag bytea NOT NULL,
  key_version integer NOT NULL,
  enabled_at timestamptz,
  last_step bigint NOT NULL DEFAULT 0,
  failed_attempts integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, user_id),
  CONSTRAINT second_factors_user_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT second_factors_failed_attempts_check CHECK (failed_attempts >= 0)
);

ALTER TABLE second_factors ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON second_factors AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON second_factors TO mimorya_app;

-- Its recovery codes: their SHA-256 alone, each used once. They go with
-- the second factor.
CREATE TABLE second_factor_recovery_codes (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL,
  code_hash bytea NOT NULL,
  used_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, user_id, code_hash),
  CONSTRAINT second_factor_recovery_codes_factor_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES second_factors (tenant_id, user_id) ON DELETE CASCADE,
  CONSTRAINT second_factor_recovery_codes_hash_check CHECK (octet_length(code_hash) = 32)
);

ALTER TABLE second_factor_recovery_codes ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON second_factor_recovery_codes AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON second_factor_recovery_codes TO mimorya_app;

-- A sign-in half done: the password checked, the second factor still to
-- prove (`verify`), or to enrol when the company requires one the account
-- has not (`enrol`). No session exists until it is done. Only the SHA-256
-- digest of its token is stored, held in a cookie of the browser; it
-- lasts minutes, serves once, and ends with the password or the account
-- that opened it. Its attempts are counted per account, not per challenge.
CREATE TABLE login_challenges (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL,
  token_hash text NOT NULL UNIQUE,
  purpose text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  CONSTRAINT login_challenges_user_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT login_challenges_purpose_check CHECK (purpose IN ('verify', 'enrol'))
);

CREATE INDEX login_challenges_user_idx ON login_challenges (tenant_id, user_id);

CREATE INDEX login_challenges_expiry_idx ON login_challenges (expires_at);

ALTER TABLE login_challenges ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON login_challenges AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON login_challenges TO mimorya_app;

-- A challenge ends as soon as the password changes, by whatever path, or
-- the account stops being active: the password it proved no longer holds.
-- Runs as the caller, under its company's row-level security.
CREATE FUNCTION retire_login_challenges()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  DELETE FROM login_challenges WHERE tenant_id = NEW.tenant_id AND user_id = NEW.id;
  RETURN NULL;
END
$$;

CREATE TRIGGER users_retire_login_challenges
  AFTER UPDATE OF password_hash, status ON users
  FOR EACH ROW
  WHEN (OLD.password_hash IS DISTINCT FROM NEW.password_hash OR NEW.status <> 'active')
  EXECUTE FUNCTION retire_login_challenges();

-- The only read before sign-in: the challenge a cookie designates, with
-- what decides whether it still holds.
CREATE FUNCTION auth_find_login_challenge(p_token_hash text)
RETURNS TABLE (
  challenge_id uuid,
  tenant_id uuid,
  user_id uuid,
  purpose text,
  expires_at timestamptz,
  user_status text,
  tenant_status text
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT c.id, c.tenant_id, c.user_id, c.purpose, c.expires_at, u.status, t.status
  FROM login_challenges c
  JOIN users u ON u.id = c.user_id AND u.tenant_id = c.tenant_id
  JOIN tenants t ON t.id = c.tenant_id
  WHERE c.token_hash = p_token_hash
$$;

REVOKE ALL ON FUNCTION auth_find_login_challenge(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_login_challenge(text) TO mimorya_app;

-- What a second factor leaves behind in every company, deleted by the
-- periodic maintenance: the challenges expired, and the enrolments left
-- waiting a day for their first code (their sealed secret with them).
CREATE FUNCTION system_purge_second_factor_leftovers()
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH challenges AS (
    DELETE FROM login_challenges WHERE expires_at <= now() RETURNING 1
  ), enrolments AS (
    DELETE FROM second_factors
    WHERE enabled_at IS NULL AND created_at <= now() - interval '1 day'
    RETURNING 1
  )
  SELECT ((SELECT count(*) FROM challenges) + (SELECT count(*) FROM enrolments))::integer
$$;

REVOKE ALL ON FUNCTION system_purge_second_factor_leftovers() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_second_factor_leftovers() TO mimorya_app;
