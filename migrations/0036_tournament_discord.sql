-- The live tournament notes posted to a Discord channel
-- (crates/yugioh/src/store/discord.rs, `DISCORD_WEBHOOK_URL`).

-- Opted in on the live start page: 1 = each round and the day's summary are
-- posted.
ALTER TABLE tournament ADD COLUMN post_to_discord INTEGER NOT NULL DEFAULT 0
    CHECK (post_to_discord IN (0, 1));

-- What was posted, so a resend posts nothing and a change edits the message.
-- Its own table: a round's row is replaced on each save.
CREATE TABLE tournament_discord_post (
    tournament_id INTEGER NOT NULL REFERENCES tournament (id) ON DELETE CASCADE,
    -- a round's number, 0 = the day's summary
    slot INTEGER NOT NULL CHECK (slot >= 0),
    content TEXT NOT NULL,
    message_id TEXT NOT NULL,
    PRIMARY KEY (tournament_id, slot)
);
