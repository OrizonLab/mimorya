-- The model that reviews a company's answers (ADR-108), chosen apart from
-- the one its conversations use: a row of its own beside theirs, its API key
-- a secret of its own (`secrets`, name `verification.api_key`). Without a
-- row, the conversations' model reviews the answers, as before.
ALTER TABLE tenant_llm_settings
  ADD COLUMN purpose text NOT NULL DEFAULT 'conversations'
  CONSTRAINT tenant_llm_settings_purpose_check CHECK (purpose IN ('conversations', 'verification'));

ALTER TABLE tenant_llm_settings DROP CONSTRAINT tenant_llm_settings_pkey;
ALTER TABLE tenant_llm_settings ADD PRIMARY KEY (tenant_id, purpose);
-- Each row says what it serves.
ALTER TABLE tenant_llm_settings ALTER COLUMN purpose DROP DEFAULT;
