-- Missions (ADR-089): work a person entrusts to their agent, carried out in
-- the background in stages, each a run of the mission's own conversation
-- under a stage's budget. A stage that spent its budget ends with a
-- progress note and, in the same transaction, queues the next one while the
-- mission's limits allow; a final answer, reviewed against the success
-- criteria, ends the mission. The table exists since the initial schema,
-- never written until now: it holds no row.
--
-- `stages` counts the stages started, `limits` the mission's own (stages and
-- cost, as when it was entrusted), `evidence` the sources its stages
-- collected, which ground the answer of the next ones. `result`,
-- `verification` and `error` say how it ended: its status is written when it
-- ends here (done, failed past its limits, cancelled), read from its last
-- stage meanwhile (awaiting a decision, failed elsewhere).
ALTER TABLE missions
  ADD COLUMN conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  ADD COLUMN limits jsonb NOT NULL,
  ADD COLUMN stages integer NOT NULL DEFAULT 1,
  ADD COLUMN evidence jsonb NOT NULL DEFAULT '[]'::jsonb,
  ADD COLUMN result text,
  ADD COLUMN verification jsonb,
  ADD COLUMN error text,
  ADD COLUMN finished_at timestamptz,
  ADD CONSTRAINT missions_title CHECK (char_length(title) BETWEEN 1 AND 120),
  ADD CONSTRAINT missions_description CHECK (char_length(description) BETWEEN 1 AND 4000),
  ADD CONSTRAINT missions_stages CHECK (stages >= 1);

-- One mission per conversation: the conversation tells its mission.
CREATE UNIQUE INDEX missions_conversation_idx ON missions (conversation_id);

-- A mission's stages, in order.
CREATE INDEX agent_runs_mission_idx ON agent_runs (mission_id, created_at)
  WHERE mission_id IS NOT NULL;
