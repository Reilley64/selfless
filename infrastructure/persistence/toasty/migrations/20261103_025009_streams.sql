ALTER TABLE "media_files" ADD COLUMN "container" TEXT;
ALTER TABLE "media_files" ADD COLUMN "duration_ms" BIGINT;
CREATE TABLE "video_streams" (
    "id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "index" BIGINT NOT NULL,
    "codec" TEXT NOT NULL,
    "profile" TEXT,
    "width" BIGINT NOT NULL,
    "height" BIGINT NOT NULL,
    "frame_rate" DOUBLE PRECISION,
    "hdr_type" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_video_streams_by_media_file_id" ON "video_streams" ("media_file_id");
CREATE TABLE "subtitle_files" (
    "id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "path" TEXT NOT NULL,
    "language" TEXT,
    "hearing_impaired" BOOLEAN NOT NULL,
    "forced" BOOLEAN NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_subtitle_files_by_media_file_id" ON "subtitle_files" ("media_file_id");
CREATE TABLE "audio_streams" (
    "id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "index" BIGINT NOT NULL,
    "codec" TEXT NOT NULL,
    "language" TEXT,
    "channels" BIGINT NOT NULL,
    "title" TEXT,
    "is_default" BOOLEAN NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_audio_streams_by_media_file_id" ON "audio_streams" ("media_file_id");
CREATE TABLE "subtitle_streams" (
    "id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "index" BIGINT NOT NULL,
    "codec" TEXT NOT NULL,
    "language" TEXT,
    "title" TEXT,
    "is_default" BOOLEAN NOT NULL,
    "forced" BOOLEAN NOT NULL,
    "hearing_impaired" BOOLEAN NOT NULL,
    "text" BOOLEAN NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_subtitle_streams_by_media_file_id" ON "subtitle_streams" ("media_file_id");
