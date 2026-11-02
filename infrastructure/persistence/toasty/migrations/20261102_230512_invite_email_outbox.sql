CREATE TYPE "invite_email_status" AS ENUM ('queued', 'sent', 'rejected');
CREATE TYPE "outbox_data" AS ENUM ('send_invite_email');
ALTER TABLE "invites" ALTER COLUMN "token_hash" DROP NOT NULL;
ALTER TABLE "invites" DROP COLUMN "sent_at";
ALTER TABLE "invites" ALTER COLUMN "expires_at" DROP NOT NULL;
ALTER TABLE "invites" ADD COLUMN "email_status_queued_at" TIMESTAMPTZ(6);
ALTER TABLE "invites" ADD COLUMN "email_status_sent_at" TIMESTAMPTZ(6);
ALTER TABLE "invites" ADD COLUMN "email_status" "invite_email_status" NOT NULL;
ALTER TABLE "invites" ADD COLUMN "email_status_rejected_at" TIMESTAMPTZ(6);
CREATE TABLE "outboxes" (
    "id" UUID NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL,
    "updated_at" TIMESTAMPTZ(6) NOT NULL,
    "delivered_at" TIMESTAMPTZ(6),
    "data" "outbox_data" NOT NULL,
    "data_invite_id" UUID,
    "data_origin" TEXT,
    "traceparent" TEXT,
    PRIMARY KEY ("id")
);
CREATE INDEX "index_outboxes_by_delivered_at_and_updated_at" ON "outboxes" ("delivered_at", "updated_at");
