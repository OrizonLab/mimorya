-- The permissions Slack says a gateway's application token has (ADR-132),
-- as it said them when the token was given or last checked: what the
-- application asks for and lacks, and what it has beyond that, are shown
-- to the company's administrators. Null when Slack did not say them, or
-- for a gateway declared before they were read; `scopes_checked_at` is
-- when Slack was last asked.
ALTER TABLE gateways
  ADD COLUMN granted_scopes text[],
  ADD COLUMN scopes_checked_at timestamptz,
  -- None, or each name as Slack writes a permission's, none empty nor
  -- null (written `!`).
  ADD CONSTRAINT gateways_granted_scopes CHECK (
    granted_scopes IS NULL OR (
      cardinality(granted_scopes) <= 200
      AND (
        cardinality(granted_scopes) = 0
        OR array_to_string(granted_scopes, ',', '!')
          ~ '^[a-z][a-z0-9:._-]{0,79}(,[a-z][a-z0-9:._-]{0,79})*$'
      )
    )
  );
