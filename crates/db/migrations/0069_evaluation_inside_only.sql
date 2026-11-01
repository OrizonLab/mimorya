-- Whether an evaluation was played without the connectors that reach
-- outside the platform (ADR-153, ADR-156): set with its first case result
-- written so, when the version evaluated is a proposal of the agent no
-- administrator made active, and shown with it.
ALTER TABLE skill_evaluations ADD COLUMN inside_only boolean NOT NULL DEFAULT false;
