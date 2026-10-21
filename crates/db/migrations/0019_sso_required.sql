-- A provider may be required for its domains (ADR-066): their accounts
-- sign in through it, never with a password, except the company's
-- administrators of its sign-in. The sign-in page learns it from the
-- discovery, which now says so.
ALTER TABLE identity_providers ADD COLUMN required boolean NOT NULL DEFAULT false;

-- Its result gains a column: dropped and created again, as in 0018 otherwise.
DROP FUNCTION auth_find_identity_provider(text);

CREATE FUNCTION auth_find_identity_provider(p_email text)
RETURNS TABLE (tenant_id uuid, provider_id uuid, kind text, name text, required boolean)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT p.tenant_id, p.id, p.kind, p.name, p.required
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
