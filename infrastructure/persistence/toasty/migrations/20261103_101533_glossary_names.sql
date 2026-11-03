ALTER TABLE "movies" RENAME COLUMN "visible" TO "in_library";
ALTER TABLE "episodes" RENAME COLUMN "visible" TO "in_library";
UPDATE "sync_problems" SET "kind" = 'missing_media_file' WHERE "kind" = 'file_not_found';
UPDATE "sync_problems" SET "kind" = 'unrecognised_media_file' WHERE "kind" = 'ffprobe_failed';
