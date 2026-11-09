-- The operator suspends and reactivates a company by its slug
-- (`mimorya-admin`, ADR-191). The company is found by its slug before any
-- transaction of its own, as the operator names it: its identifier alone,
-- or nothing. The change itself runs in the company's own transaction,
-- under its row-level security, with its audit entry.
CREATE FUNCTION system_find_tenant(p_slug text)
RETURNS uuid
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT tenants.id FROM tenants WHERE tenants.slug = p_slug
$$;

REVOKE ALL ON FUNCTION system_find_tenant(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_find_tenant(text) TO mimorya_app;
