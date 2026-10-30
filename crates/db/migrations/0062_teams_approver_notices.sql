-- Requests awaiting a manager announced in Teams too (ADR-142): the
-- manager, or whom they named to stand in for them, linked to the
-- company's Teams bot, is written in their personal conversation with it,
-- as in Slack (ADR-130) and Telegram (ADR-137), the actions that may be
-- validated there shown on a card with their buttons (ADR-141).

-- The bot writes only to a conversation Teams knows: the personal
-- conversation of a linked person with it (its identifier, `a:…`), at the
-- Teams service its latest activity came from. Kept with the identity: the
-- one the link was sent in when it is confirmed, then the one each of the
-- person's messages comes from. Only for a Teams identity, both or
-- neither (an identity linked before keeps none until the person writes
-- to the bot again), as the platform writes them (0060).
ALTER TABLE gateway_identities
  ADD COLUMN service_url text,
  ADD COLUMN conversation_id text,
  ADD CONSTRAINT gateway_identities_teams_conversation CHECK (
    (service_url IS NULL AND conversation_id IS NULL)
    OR (kind = 'teams'
      AND service_url IS NOT NULL AND conversation_id IS NOT NULL
      AND service_url ~ '^https?://[!-~]+$' AND char_length(service_url) <= 200
      AND conversation_id ~ '^[!-~]+$' AND char_length(conversation_id) <= 512)
  );

-- A card is written either with the pause of a conversation held in a
-- thread (ADR-141), or with a notice to a manager (`user_id`, whom it was
-- written to): no thread then. Each names its bot and the personal
-- conversation it was written in, where alone its buttons decide; a
-- notice's card goes with the bot, or with whom it was written to.
ALTER TABLE gateway_cards
  ADD COLUMN gateway_id uuid REFERENCES gateways(id) ON DELETE CASCADE,
  ADD COLUMN channel_id text,
  ADD COLUMN user_id uuid REFERENCES users(id) ON DELETE CASCADE,
  ALTER COLUMN thread_id DROP NOT NULL;

UPDATE gateway_cards c SET gateway_id = t.gateway_id, channel_id = t.channel_id
  FROM gateway_threads t WHERE t.id = c.thread_id;

ALTER TABLE gateway_cards
  ALTER COLUMN gateway_id SET NOT NULL,
  ALTER COLUMN channel_id SET NOT NULL,
  DROP CONSTRAINT gateway_cards_once,
  ADD CONSTRAINT gateway_cards_once UNIQUE (gateway_id, channel_id, activity_id),
  ADD CONSTRAINT gateway_cards_channel
    CHECK (channel_id ~ '^[!-~]+$' AND char_length(channel_id) <= 512),
  ADD CONSTRAINT gateway_cards_written_with
    CHECK ((thread_id IS NULL) <> (user_id IS NULL));

-- Forgets, in every company, the cards of notices written before
-- `p_before` (their requests no longer await anyone by then), and those
-- written in a conversation that is no longer where their manager is
-- written (their Teams account unlinked, or another linked since); how
-- many. Only identifiers are kept, never what a card showed.
CREATE FUNCTION system_purge_notice_cards(p_before timestamptz)
RETURNS bigint
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM gateway_cards c
     WHERE c.user_id IS NOT NULL
       AND (c.created_at < p_before
         OR NOT EXISTS (
           SELECT 1 FROM gateway_identities i
            WHERE i.gateway_id = c.gateway_id
              AND i.user_id = c.user_id
              AND i.conversation_id = c.channel_id))
    RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_notice_cards(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_notice_cards(timestamptz) TO mimorya_app;

-- The paused runs, of every active company whose Slack application,
-- Telegram bot or Teams bot is enabled, with requests made since `p_since`
-- awaiting a manager whose account is active and whose notice through that
-- gateway is still to try at `p_now`: as 0058, Teams added.
DROP FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer);

CREATE FUNCTION system_gateway_approvals_to_notify(
  p_since timestamptz, p_now timestamptz, p_limit integer)
RETURNS TABLE (tenant_id uuid, run_id uuid, gateway_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT a.tenant_id, a.run_id, g.id
    FROM approvals a
    JOIN tenants t ON t.id = a.tenant_id AND t.status = 'active'
    JOIN gateways g ON g.tenant_id = a.tenant_id
     AND g.kind IN ('slack', 'telegram', 'teams')
     AND g.enabled
    JOIN agent_runs r ON r.id = a.run_id AND r.tenant_id = a.tenant_id
    JOIN users m ON m.id = a.approver_user_id AND m.status = 'active'
   WHERE a.status = 'pending'
     AND a.routing = 'manager'
     AND a.created_at > p_since
     AND r.status = 'awaiting_approval'
     AND NOT EXISTS (
       SELECT 1 FROM gateway_approval_notices n
        WHERE n.approval_id = a.id
          AND n.gateway_id = g.id
          AND (n.outcome IS NOT NULL OR n.retry_after > p_now))
   GROUP BY a.tenant_id, a.run_id, g.id
   ORDER BY min(a.created_at)
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) TO mimorya_app;
