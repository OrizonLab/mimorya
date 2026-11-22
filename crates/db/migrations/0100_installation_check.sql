-- What the operator's installation check reads of the migration journal
-- (ADR-223): each version applied, whether it succeeded and its checksum,
-- to compare with the migrations the running version carries. Nothing
-- else: the journal itself stays the owner's, unreadable by the
-- application role.
CREATE FUNCTION system_applied_migrations()
RETURNS TABLE (version bigint, success boolean, checksum bytea)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version
$$;

REVOKE ALL ON FUNCTION system_applied_migrations() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_applied_migrations() TO mimorya_app;
