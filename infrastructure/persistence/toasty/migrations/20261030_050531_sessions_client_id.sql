ALTER TABLE "sessions" ADD COLUMN "client_id" UUID NOT NULL;
CREATE UNIQUE INDEX "index_sessions_by_user_id_and_client_id" ON "sessions" ("user_id", "client_id");
