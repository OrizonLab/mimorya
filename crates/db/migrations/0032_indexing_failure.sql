-- Why the last pass of a company's indexing failed (ADR-074), kept with its
-- choice of model rather than in the memory of the process that indexes:
-- a worker indexes, an `api` process shows it to the administrators
-- (ADR-085). Forgotten when a pass succeeds and when the choice changes.
ALTER TABLE tenant_embedding_settings ADD COLUMN indexing_failure text;
