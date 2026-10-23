-- Who declared an absence on the absent person's behalf (ADR-078): an
-- account administrator, for a collaborator who cannot declare it. Null when
-- the absent person declared it, or once the administrator's account is
-- deleted; declared again by the absent person, it is null again.
ALTER TABLE approval_delegations
  ADD COLUMN declared_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL;
