ALTER TABLE "inbox_rows" ADD COLUMN "error" TEXT;
ALTER TABLE "inbox_rows" ADD COLUMN "attempts" BIGINT NOT NULL;
ALTER TABLE "inbox_rows" ADD COLUMN "next_attempt_at" TIMESTAMPTZ(6) NOT NULL;
ALTER TABLE "inbox_rows" ADD COLUMN "source_item_id" BIGINT;
