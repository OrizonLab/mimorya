-- The e-mail domains a company proved it holds (ADR-069): a TXT record of
-- its own, published in the domain's DNS, was found there. A generic OpenID
-- Connect or SAML provider vouches for whatever addresses its owner wants:
-- it creates accounts only in these domains, since an address, unique across
-- companies, would otherwise be taken from the company that holds it.
CREATE TABLE verified_domains (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  domain citext NOT NULL,
  verified_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  verified_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, domain)
);

ALTER TABLE verified_domains ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON verified_domains AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE ON verified_domains TO mimorya_app;

-- Whether another company proved `p_domain` while `p_tenant_id` did not: an
-- address of the domain is then the other's, and no new account of
-- `p_tenant_id` takes it, by invitation or at a sign-in. Reads across
-- companies, answering yes or no only, never which company.
CREATE FUNCTION auth_domain_held_elsewhere(p_tenant_id uuid, p_domain text)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT EXISTS (
    SELECT 1 FROM verified_domains WHERE domain = p_domain::citext AND tenant_id <> p_tenant_id
  ) AND NOT EXISTS (
    SELECT 1 FROM verified_domains WHERE domain = p_domain::citext AND tenant_id = p_tenant_id
  )
$$;

REVOKE ALL ON FUNCTION auth_domain_held_elsewhere(uuid, text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_domain_held_elsewhere(uuid, text) TO mimorya_app;
