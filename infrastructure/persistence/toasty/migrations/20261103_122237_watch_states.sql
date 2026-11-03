CREATE TABLE "episode_watch_states" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "episode_id" UUID NOT NULL,
    "resume_position_seconds" BIGINT,
    "watched" BOOLEAN NOT NULL,
    "watched_at" TIMESTAMPTZ(6),
    "last_played" TIMESTAMPTZ(6),
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_episode_watch_states_by_user_id_and_episode_id" ON "episode_watch_states" ("user_id", "episode_id");
CREATE INDEX "index_episode_watch_states_by_episode_id" ON "episode_watch_states" ("episode_id");
CREATE TABLE "movie_watch_states" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "movie_id" UUID NOT NULL,
    "resume_position_seconds" BIGINT,
    "watched" BOOLEAN NOT NULL,
    "watched_at" TIMESTAMPTZ(6),
    "last_played" TIMESTAMPTZ(6),
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_movie_watch_states_by_user_id_and_movie_id" ON "movie_watch_states" ("user_id", "movie_id");
CREATE INDEX "index_movie_watch_states_by_movie_id" ON "movie_watch_states" ("movie_id");
