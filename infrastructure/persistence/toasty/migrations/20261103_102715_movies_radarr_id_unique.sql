DROP INDEX "index_movies_by_radarr_id";
CREATE UNIQUE INDEX "index_movies_by_radarr_id" ON "movies" ("radarr_id");
