ALTER TABLE "video_streams" ADD COLUMN "interlaced" BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE "video_streams" ALTER COLUMN "interlaced" DROP DEFAULT;
ALTER TABLE "video_streams" ADD COLUMN "bit_rate" BIGINT;
UPDATE "media_files" SET "modified_at" = NULL;
