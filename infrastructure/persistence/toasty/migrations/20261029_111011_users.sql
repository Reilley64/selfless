CREATE TYPE "colour" AS ENUM ('amber', 'blue', 'pink', 'emerald');
CREATE TYPE "symbol" AS ENUM ('ring', 'triangle', 'star', 'waves');
CREATE TYPE "client_kind" AS ENUM ('web', 'appleTv', 'googleTv');
CREATE TABLE "users" (
    "id" UUID NOT NULL,
    "name" TEXT COLLATE "case_insensitive" NOT NULL,
    "password_hash" TEXT NOT NULL,
    "colour" "colour" NOT NULL,
    "symbol" "symbol" NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_users_by_name" ON "users" ("name");
CREATE TABLE "sessions" (
    "id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "client_kind" "client_kind" NOT NULL,
    "device_name" TEXT NOT NULL,
    "token_hash" BYTEA NOT NULL,
    "signed_in_at" TIMESTAMPTZ(6) NOT NULL,
    "last_used_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_sessions_by_user_id" ON "sessions" ("user_id");
CREATE UNIQUE INDEX "index_sessions_by_token_hash" ON "sessions" ("token_hash");
