CREATE TABLE "source_states" (
    "source" TEXT NOT NULL,
    "last_webhook_at" TIMESTAMPTZ(6),
    "rejected_webhooks" BIGINT NOT NULL,
    PRIMARY KEY ("source")
);
CREATE TABLE "inbox_rows" (
    "id" UUID NOT NULL,
    "source" TEXT NOT NULL,
    "event_type" TEXT,
    "body" BYTEA NOT NULL,
    "received_at" TIMESTAMPTZ(6) NOT NULL,
    "traceparent" TEXT,
    "status" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_inbox_rows_by_status" ON "inbox_rows" ("status");
