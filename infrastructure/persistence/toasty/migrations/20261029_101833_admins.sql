CREATE TABLE "admin_sessions" (
    "id" UUID NOT NULL,
    "admin_id" UUID NOT NULL,
    "token_hash" BYTEA NOT NULL,
    "signed_in_at" TIMESTAMPTZ(6) NOT NULL,
    "last_used_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_admin_sessions_by_admin_id" ON "admin_sessions" ("admin_id");
CREATE UNIQUE INDEX "index_admin_sessions_by_token_hash" ON "admin_sessions" ("token_hash");
CREATE COLLATION "case_insensitive" (provider = icu, locale = 'und-u-ks-level2', deterministic = false);
CREATE TABLE "admins" (
    "id" UUID NOT NULL,
    "name" TEXT COLLATE "case_insensitive" NOT NULL,
    "password_hash" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_admins_by_name" ON "admins" ("name");
