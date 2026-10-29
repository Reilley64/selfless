CREATE TABLE "movies" (
    "id" UUID NOT NULL,
    "tmdb_id" BIGINT NOT NULL,
    "radarr_id" BIGINT NOT NULL,
    "title" TEXT NOT NULL,
    "year" BIGINT NOT NULL,
    "overview" TEXT,
    "genres" TEXT[] NOT NULL,
    "added_at" TIMESTAMPTZ(6),
    "visible" BOOLEAN NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_movies_by_tmdb_id" ON "movies" ("tmdb_id");
CREATE INDEX "index_movies_by_radarr_id" ON "movies" ("radarr_id");
