ALTER TABLE "media_files" ADD COLUMN "modified_at" TIMESTAMPTZ(6);
ALTER TABLE "media_files" ADD COLUMN "size" BIGINT;
ALTER TABLE "movies" ADD COLUMN "media_file_id" UUID;
CREATE INDEX "index_movies_by_media_file_id" ON "movies" ("media_file_id");
ALTER TABLE "source_states" ADD COLUMN "last_reconcile_duration_ms" BIGINT;
ALTER TABLE "source_states" ADD COLUMN "last_reconcile_started_at" TIMESTAMPTZ(6);
ALTER TABLE "source_states" ADD COLUMN "last_reconcile_result" TEXT;
CREATE TABLE "held_removals" (
    "source" TEXT NOT NULL,
    "item_ids" UUID[] NOT NULL,
    PRIMARY KEY ("source")
);
