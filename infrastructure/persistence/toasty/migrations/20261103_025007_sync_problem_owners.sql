ALTER TABLE "sync_problems" ALTER COLUMN "movie_id" DROP NOT NULL;
ALTER TABLE "sync_problems" ADD COLUMN "media_file_id" UUID;
CREATE INDEX "index_sync_problems_by_media_file_id" ON "sync_problems" ("media_file_id");
