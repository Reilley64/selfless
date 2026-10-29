ALTER TABLE "users" ADD COLUMN "email" TEXT COLLATE "case_insensitive" NOT NULL;
CREATE UNIQUE INDEX "index_users_by_email" ON "users" ("email");
CREATE TABLE "invites" (
    "id" UUID NOT NULL,
    "email" TEXT COLLATE "case_insensitive" NOT NULL,
    "token_hash" BYTEA NOT NULL,
    "admin_id" UUID NOT NULL,
    "sent_at" TIMESTAMPTZ(6) NOT NULL,
    "expires_at" TIMESTAMPTZ(6) NOT NULL,
    PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "index_invites_by_email" ON "invites" ("email");
CREATE UNIQUE INDEX "index_invites_by_token_hash" ON "invites" ("token_hash");
