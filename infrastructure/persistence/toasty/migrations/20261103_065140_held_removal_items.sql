ALTER TABLE "held_removals" DROP COLUMN "item_ids";
CREATE TABLE "held_removal_items" (
    "id" UUID NOT NULL,
    "source" TEXT NOT NULL,
    "item_id" UUID NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_held_removal_items_by_source" ON "held_removal_items" ("source");
