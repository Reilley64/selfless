DROP TABLE "inbox_rows";
CREATE TABLE "inboxes" (
    "id" UUID NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL,
    "updated_at" TIMESTAMPTZ(6) NOT NULL,
    "delivered_at" TIMESTAMPTZ(6),
    "source" TEXT NOT NULL,
    "body" BYTEA NOT NULL,
    "received_at" TIMESTAMPTZ(6) NOT NULL,
    "traceparent" TEXT,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_inboxes_by_delivered_at_and_updated_at" ON "inboxes" ("delivered_at", "updated_at");
