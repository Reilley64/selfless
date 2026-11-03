CREATE TABLE "show_continue_watching_removals" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "show_id" UUID NOT NULL,
    "removed_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_show_continue_watching_removals_by_user_id_and_show_id" ON "show_continue_watching_removals" ("user_id", "show_id");
CREATE TABLE "movie_continue_watching_removals" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "movie_id" UUID NOT NULL,
    "removed_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_movie_continue_watching_removals_by_user_id_and_movie_id" ON "movie_continue_watching_removals" ("user_id", "movie_id");
