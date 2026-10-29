CREATE TABLE "inbox_attempts" (
    "id" UUID NOT NULL,
    "inbox_row_id" UUID NOT NULL,
    "attempted_at" TIMESTAMPTZ(6) NOT NULL,
    "error" TEXT,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_inbox_attempts_by_inbox_row_id" ON "inbox_attempts" ("inbox_row_id");
