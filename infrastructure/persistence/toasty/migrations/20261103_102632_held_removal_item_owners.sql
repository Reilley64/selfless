ALTER TABLE "held_removal_items" ADD COLUMN "show_id" UUID;
ALTER TABLE "held_removal_items" ADD COLUMN "movie_id" UUID;
UPDATE "held_removal_items" SET "movie_id" = "item_id" WHERE "source" = 'radarr';
UPDATE "held_removal_items" SET "show_id" = "item_id" WHERE "source" = 'sonarr';
ALTER TABLE "held_removal_items" DROP COLUMN "item_id";
