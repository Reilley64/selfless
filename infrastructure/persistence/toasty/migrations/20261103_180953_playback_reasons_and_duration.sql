ALTER TABLE "playbacks" ADD COLUMN "duration" DOUBLE PRECISION NOT NULL DEFAULT 0;
ALTER TABLE "playbacks" ALTER COLUMN "duration" DROP DEFAULT;
CREATE TABLE "playback_reasons" (
    "id" UUID NOT NULL,
    "playback_id" UUID NOT NULL,
    "index" BIGINT NOT NULL,
    "kind" TEXT NOT NULL,
    "value" TEXT,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_playback_reasons_by_playback_id" ON "playback_reasons" ("playback_id");
