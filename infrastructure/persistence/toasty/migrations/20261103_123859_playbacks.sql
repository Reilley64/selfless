CREATE TABLE "playbacks" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "method" TEXT NOT NULL,
    "position" DOUBLE PRECISION NOT NULL,
    "paused" BOOLEAN NOT NULL,
    "started_at" TIMESTAMPTZ(6) NOT NULL,
    "last_reported_at" TIMESTAMPTZ(6) NOT NULL,
    "ended_at" TIMESTAMPTZ(6),
    PRIMARY KEY ("id")
);
CREATE INDEX "index_playbacks_by_user_id" ON "playbacks" ("user_id");
CREATE INDEX "index_playbacks_by_session_id" ON "playbacks" ("session_id");
