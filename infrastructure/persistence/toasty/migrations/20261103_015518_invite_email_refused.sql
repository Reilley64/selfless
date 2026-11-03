ALTER TYPE "invite_email_status" RENAME VALUE 'rejected' TO 'refused';
ALTER TABLE "invites" RENAME COLUMN "email_status_rejected_at" TO "email_status_refused_at";
