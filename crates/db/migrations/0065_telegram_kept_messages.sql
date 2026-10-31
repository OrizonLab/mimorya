-- A Telegram message written again once its actions are settled (ADR-148):
-- the bot's message in a person's private chat that shows actions with
-- their buttons, a pause's (ADR-136) or a notice's to the manager a
-- request awaits (ADR-137), is kept as a Teams card is (0061, 0062): its
-- bot, the private chat (the person's Telegram identifier), the message's
-- number as Telegram gave it (`activity_id`), the requests it shows and
-- how many others it said of. Only identifiers and counts, never what was
-- written. Its buttons decide as before, on what Telegram holds of them
-- (ADR-136): what is kept serves only to write it again.

-- Forgets, in every company, the cards of notices written before
-- `p_before` (their requests no longer await anyone by then), and those no
-- longer where their manager is written: a Teams card in a conversation
-- that is no longer theirs (0062); a Telegram message in a private chat
-- that is no longer the Telegram account linked to them (unlinked, or
-- another linked since). How many.
CREATE OR REPLACE FUNCTION system_purge_notice_cards(p_before timestamptz)
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
              AND ((i.kind = 'teams' AND i.conversation_id = c.channel_id)
                OR (i.kind = 'telegram' AND i.external_id = c.channel_id))))
    RETURNING 1
  )
  SELECT count(*) FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_notice_cards(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_notice_cards(timestamptz) TO mimorya_app;
