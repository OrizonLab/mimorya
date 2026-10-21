-- An invited account may accept its invitation through its company's
-- identity provider (ADR-064): the sign-in page, and the invitation's own,
-- offer the provider to it too. Otherwise as in 0017: the provider of the
-- account with this address, if its company is active and has an enabled
-- provider covering the address's domain; a disabled account reads as none.
CREATE OR REPLACE FUNCTION auth_find_identity_provider(p_email text)
RETURNS TABLE (tenant_id uuid, provider_id uuid, kind text, name text)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT p.tenant_id, p.id, p.kind, p.name
    FROM users u
    JOIN tenants t ON t.id = u.tenant_id
    JOIN identity_provider_domains d
      ON d.tenant_id = u.tenant_id AND d.domain = split_part(u.email::text, '@', 2)::citext
    JOIN identity_providers p ON p.id = d.provider_id AND p.tenant_id = d.tenant_id
   WHERE u.email = p_email::citext
     AND u.status IN ('active', 'invited')
     AND p.enabled
     AND t.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_find_identity_provider(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_identity_provider(text) TO mimorya_app;

-- Whether the account was ever active: its invitation accepted, by its link
-- or through the provider, or created with a temporary password. Re-enabled,
-- an account that never was goes back to `invited` (ADR-027); one accepted
-- through the provider has no password but stays active (ADR-064). Until
-- now an account without a password had never been active.
ALTER TABLE users ADD COLUMN activated boolean NOT NULL DEFAULT true;

UPDATE users SET activated = false WHERE password_hash IS NULL;
