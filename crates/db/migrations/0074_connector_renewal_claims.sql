-- The renewal of an MCP server's OAuth access token claimed by one process
-- for a while (ADR-171): a single statement claims it, short, no lock held
-- during the call to the token endpoint; another process sees it claimed
-- and starts its session with the current token. Until the claim ends, a
-- token endpoint that did not answer is not asked again: the claim is also
-- the delay before the next attempt. A renewal kept, or the authorization
-- replaced, clears it.
ALTER TABLE connector_authorizations ADD COLUMN renewing_until timestamptz;
