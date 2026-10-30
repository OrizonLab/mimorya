-- Validations given in Teams (ADR-141): the card the company's Teams bot
-- shows a pause's actions on, with their « Valider » and « Refuser »
-- buttons, in the person's personal conversation with it. A button pressed
-- comes back as a message carrying what the button carries, which the
-- client writes, and naming the card it was pressed on (`replyToId`), not
-- what that card showed: the platform keeps, for each card it wrote, which
-- requests it shows, so that only a request shown on a card of its own, in
-- that conversation, is decided there.

-- A card the bot wrote in the conversation held in a thread: its activity
-- (as the Bot Connector numbered it when it took the card), the requests it
-- shows, from one to five, in order, and how many others its pause said
-- are to validate in the application, or await a manager: the card is
-- written again as it was, once some of its requests are decided. Only
-- identifiers and counts, never what was written: that is the requests
-- themselves. A card goes with its thread.
CREATE TABLE gateway_cards (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  thread_id uuid NOT NULL REFERENCES gateway_threads(id) ON DELETE CASCADE,
  activity_id text NOT NULL,
  approval_ids uuid[] NOT NULL,
  in_application integer NOT NULL DEFAULT 0,
  managed integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_cards_others
    CHECK (in_application BETWEEN 0 AND 1000 AND managed BETWEEN 0 AND 1000),
  CONSTRAINT gateway_cards_activity
    CHECK (activity_id ~ '^[!-~]+$' AND char_length(activity_id) <= 512),
  CONSTRAINT gateway_cards_approvals
    CHECK (cardinality(approval_ids) BETWEEN 1 AND 5
      AND array_ndims(approval_ids) = 1
      AND array_position(approval_ids, NULL) IS NULL),
  CONSTRAINT gateway_cards_once UNIQUE (thread_id, activity_id)
);

ALTER TABLE gateway_cards ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_cards AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT ON gateway_cards TO mimorya_app;
