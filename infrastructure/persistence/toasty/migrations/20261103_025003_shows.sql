CREATE TABLE "seasons" (
    "id" UUID NOT NULL,
    "show_id" UUID NOT NULL,
    "number" BIGINT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_seasons_by_show_id" ON "seasons" ("show_id");
CREATE TABLE "shows" (
    "id" UUID NOT NULL,
    "tvdb_id" BIGINT NOT NULL,
    "sonarr_id" BIGINT NOT NULL,
    "title" TEXT NOT NULL,
    "year" BIGINT,
    "genres" TEXT[] NOT NULL,
    "overview" TEXT,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_shows_by_tvdb_id" ON "shows" ("tvdb_id");
CREATE UNIQUE INDEX "index_shows_by_sonarr_id" ON "shows" ("sonarr_id");
CREATE TABLE "media_files" (
    "id" UUID NOT NULL,
    "path" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_media_files_by_path" ON "media_files" ("path");
CREATE TABLE "episodes" (
    "id" UUID NOT NULL,
    "season_id" UUID NOT NULL,
    "tvdb_id" BIGINT NOT NULL,
    "number" BIGINT NOT NULL,
    "title" TEXT NOT NULL,
    "air_date" DATE,
    "overview" TEXT,
    "added_at" TIMESTAMPTZ(6),
    "media_file_id" UUID,
    "visible" BOOLEAN NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_episodes_by_season_id" ON "episodes" ("season_id");
CREATE UNIQUE INDEX "index_episodes_by_tvdb_id" ON "episodes" ("tvdb_id");
CREATE INDEX "index_episodes_by_media_file_id" ON "episodes" ("media_file_id");
