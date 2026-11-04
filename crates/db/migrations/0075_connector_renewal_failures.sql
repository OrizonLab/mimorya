-- Why the last renewal of an MCP server's OAuth access token failed while
-- the authorization was kept (ADR-171): the server did not answer, or
-- refused for another reason than a grant no longer good. Shown to the
-- connector administrators, never a token: the platform's own words, or the
-- server's error code. A renewal kept clears it; a new authorization starts
-- without one.
ALTER TABLE connector_authorizations
    ADD COLUMN renewal_failure text
        CHECK (char_length(renewal_failure) BETWEEN 1 AND 300);
