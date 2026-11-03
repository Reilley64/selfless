CREATE TABLE "keyframe_indices" (
    "id" UUID NOT NULL,
    "media_file_id" UUID NOT NULL,
    "size" BIGINT,
    "modified_at" TIMESTAMPTZ(6),
    "keyframes_ns" BIGINT[] NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_keyframe_indices_by_media_file_id" ON "keyframe_indices" ("media_file_id");
