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

CREATE INDEX "channel_nick" ON "message" ("channel", "nick");
CREATE INDEX "channel_opcode" ON "message" ("channel", "opcode");
CREATE INDEX "channel_ts" ON "message" ("channel", "timestamp");
CREATE INDEX "messages_line_fts" ON "message" USING gin (channel, to_tsvector('english', nick || ' ' || line));

CREATE OR REPLACE FUNCTION new_message_notify() RETURNS TRIGGER AS
$$
begin
    PERFORM pg_notify('new_message', row_to_json(NEW)::text);
    RETURN NEW;
end
$$ LANGUAGE plpgsql;

CREATE TRIGGER "new_message_trigger"
    AFTER INSERT
    ON "message"
    FOR EACH ROW
EXECUTE PROCEDURE new_message_notify();
