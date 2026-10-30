-- Conversations with the agent through a company's Teams bot (ADR-140): a
-- linked person writes to the bot in their personal conversation with it,
-- and is answered there.

-- In Teams, the person's personal conversation with the bot (its
-- identifier, `a:…`) holds one conversation at a time, as a private chat
-- with a Telegram bot does (0057): the one their message opened
-- (`thread_ts` is that activity's identifier), followed by their next
-- messages until they ask for a new one (`/nouvelle`), which closes it
-- (`closed_at`). Who wrote is their Microsoft Entra object. The bot answers
-- at the Teams service the activities came from (`service_url`), the
-- latest one: Microsoft may move a conversation to another. Only a Teams
-- thread has one.
ALTER TABLE gateway_threads ADD COLUMN service_url text;

ALTER TABLE gateway_threads
  DROP CONSTRAINT gateway_threads_of_kind,
  ADD CONSTRAINT gateway_threads_of_kind CHECK (
    (kind = 'slack' AND external_id ~ '^[UW][A-Z0-9]{2,30}$'
      AND channel_id ~ '^D[A-Z0-9]{2,30}$'
      AND thread_ts ~ '^[0-9]{1,12}\.[0-9]{1,9}$'
      AND closed_at IS NULL
      AND service_url IS NULL)
    OR (kind = 'telegram' AND external_id ~ '^[1-9][0-9]{0,19}$'
      AND channel_id = external_id
      AND thread_ts ~ '^[1-9][0-9]{0,19}$'
      AND service_url IS NULL)
    OR (kind = 'teams'
      AND external_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
      AND channel_id ~ '^[!-~]+$' AND char_length(channel_id) <= 512
      AND thread_ts ~ '^[!-~]{1,64}$'
      AND service_url ~ '^https?://[!-~]{1,200}$')
  );

DROP INDEX gateway_threads_one_open_chat;
CREATE UNIQUE INDEX gateway_threads_one_open_chat
  ON gateway_threads (gateway_id, channel_id, user_id)
  WHERE kind IN ('telegram', 'teams') AND closed_at IS NULL;

GRANT UPDATE (service_url) ON gateway_threads TO mimorya_app;
