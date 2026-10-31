-- A conversation's summary (ADR-145): the model folds its older messages
-- into it once they are too many or too long to be replayed word for word.
-- The history replayed to the model is this summary, then the messages after
-- the last one it covers. One per conversation, gone with it (or with the
-- message it stops at); never shown to anyone but the conversation's owner.
CREATE TABLE conversation_summaries (
  conversation_id uuid PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  content text NOT NULL,
  -- The last message it covers.
  through_message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  -- Messages folded in so far, all passes together.
  folded_messages integer NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT conversation_summaries_content CHECK (char_length(content) BETWEEN 1 AND 8000),
  CONSTRAINT conversation_summaries_folded CHECK (folded_messages > 0)
);

CREATE INDEX conversation_summaries_tenant_idx ON conversation_summaries (tenant_id);

ALTER TABLE conversation_summaries ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON conversation_summaries AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE ON conversation_summaries TO mimorya_app;
