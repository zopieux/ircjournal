CREATE EXTENSION IF NOT EXISTS btree_gin;

CREATE TABLE "message"
(
    "id"        serial PRIMARY KEY NOT NULL,
    "channel"   text,
    "nick"      text,
    "line"      text,
    "opcode"    text,
    "oper_nick" text,
    "payload"   text,
    "timestamp" timestamptz        NOT NULL
);

CREATE INDEX "ts" ON "message" ("timestamp");
CREATE INDEX "channel_nick" ON "message" ("channel", "nick");
CREATE INDEX "channel_opcode" ON "message" ("channel", "opcode");
CREATE INDEX "channel_ts" ON "message" ("channel", "timestamp");
CREATE INDEX "channel_line_fts" ON "message" USING gin (channel, to_tsvector('english', nick || ' ' || line));

-- https://wiki.postgresql.org/wiki/Loose_indexscan
CREATE OR REPLACE FUNCTION all_nicks(chan text, n numeric)
    RETURNS TABLE
            (
                nick text
            )
AS
$$
WITH RECURSIVE t AS (
    SELECT min(nick) AS nick, 1 AS cnt
    FROM message
    WHERE channel = chan AND 1 <= n
    UNION ALL
    SELECT (SELECT min(nick) FROM message WHERE nick > t.nick AND channel = chan), cnt + 1 AS cnt
    FROM t
    WHERE t.nick IS NOT NULL AND cnt < n
)
SELECT nick
FROM t
WHERE nick IS NOT NULL
$$ LANGUAGE sql;

CREATE OR REPLACE FUNCTION all_channels()
    RETURNS TABLE
            (
                channel text
            )
AS
$$
WITH RECURSIVE t AS (
    SELECT min(channel) AS channel
    FROM message
    UNION ALL
    SELECT (SELECT min(channel) FROM message WHERE channel > t.channel)
    FROM t
    WHERE t.channel IS NOT NULL
)
SELECT channel
FROM t
WHERE channel IS NOT NULL
$$ LANGUAGE sql;
