CREATE TABLE "playback_settings" (
    "user_id" UUID NOT NULL,
    "autoplay_next_episode" BOOLEAN NOT NULL,
    "ask_if_still_watching" BOOLEAN NOT NULL,
    "audio_language" TEXT,
    "subtitle_language" TEXT,
    PRIMARY KEY ("user_id")
);
