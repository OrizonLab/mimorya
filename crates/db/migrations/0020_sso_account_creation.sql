-- A provider may create, at their first sign-in, the account of a person of
-- its domains whom the company has not invited (ADR-067). That sign-in starts
-- from the company's own sign-in link, which names the provider: never from
-- an address alone, since a company declares its domains without proving
-- them and two companies may declare the same one.
ALTER TABLE identity_providers ADD COLUMN create_accounts boolean NOT NULL DEFAULT false;

-- The provider a company's sign-in link names, before sign-in: enabled, in an
-- active company, with the company's name, which the page shows so that the
-- person knows where they sign in, and whether a first sign-in creates the
-- account.
CREATE FUNCTION auth_find_identity_provider_link(p_provider_id uuid)
RETURNS TABLE (tenant_id uuid, provider_id uuid, kind text, name text, required boolean,
               create_accounts boolean, company text)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT p.tenant_id, p.id, p.kind, p.name, p.required, p.create_accounts, t.name
    FROM identity_providers p
    JOIN tenants t ON t.id = p.tenant_id
   WHERE p.id = p_provider_id
     AND p.enabled
     AND t.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_find_identity_provider_link(uuid) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_identity_provider_link(uuid) TO mimorya_app;
