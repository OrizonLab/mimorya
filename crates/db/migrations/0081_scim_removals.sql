-- An account the company's identity provider removed through SCIM
-- (ADR-183): disabled, and no longer there for SCIM (`removed_at`) until an
-- administrator enables it again. Such an account may have no identifier
-- from the provider.
ALTER TABLE scim_accounts ALTER COLUMN external_id DROP NOT NULL;
ALTER TABLE scim_accounts ADD COLUMN removed_at timestamptz;
