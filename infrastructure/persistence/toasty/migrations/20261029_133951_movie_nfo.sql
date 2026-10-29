ALTER TABLE "movies" ADD COLUMN "studio" TEXT;
ALTER TABLE "movies" ADD COLUMN "premiered" DATE;
ALTER TABLE "movies" ADD COLUMN "runtime_minutes" BIGINT;
ALTER TABLE "movies" ADD COLUMN "nfo_modified_at" TIMESTAMPTZ(6);
ALTER TABLE "movies" ADD COLUMN "certification" TEXT;
ALTER TABLE "movies" ADD COLUMN "sort_title" TEXT;
ALTER TABLE "movies" ADD COLUMN "original_title" TEXT;
ALTER TABLE "movies" ADD COLUMN "directors" TEXT[] NOT NULL;
ALTER TABLE "movies" ADD COLUMN "rating" DOUBLE PRECISION;
CREATE TABLE "sync_problems" (
    "id" UUID NOT NULL,
    "movie_id" UUID NOT NULL,
    "kind" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_sync_problems_by_movie_id" ON "sync_problems" ("movie_id");
CREATE TABLE "actors" (
    "id" UUID NOT NULL,
    "movie_id" UUID NOT NULL,
    "name" TEXT NOT NULL,
    "role" TEXT,
    "order" BIGINT,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_actors_by_movie_id" ON "actors" ("movie_id");
CREATE TABLE "artworks" (
    "id" UUID NOT NULL,
    "movie_id" UUID NOT NULL,
    "kind" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "modified_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_artworks_by_movie_id" ON "artworks" ("movie_id");
