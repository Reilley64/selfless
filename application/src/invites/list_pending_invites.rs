mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::EmailStatus;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::InviteEmailStatus;
use std::cmp::Reverse;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let invites = db_context
        .invites()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let mut invites = invites
        .into_iter()
        .map(|x| output::Invite {
            id: x.id,
            email: x.email.to_string(),
            email_status: match x.email_status {
                InviteEmailStatus::Queued { queued_at } => EmailStatus::Queued { queued_at },
                InviteEmailStatus::Sent { sent_at } => EmailStatus::Sent { sent_at },
                InviteEmailStatus::Rejected { rejected_at } => {
                    EmailStatus::Rejected { rejected_at }
                }
            },
            expires_at: x.expires_at,
        })
        .collect::<Vec<_>>();
    invites.sort_by_key(|x| {
        let (EmailStatus::Queued { queued_at: at }
        | EmailStatus::Sent { sent_at: at }
        | EmailStatus::Rejected { rejected_at: at }) = x.email_status;
        Reverse(at)
    });
    Ok(Output { invites })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::EmailAddress;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use uuid::Uuid;

    async fn seed(
        db_context: &mut DbContext,
        email: &str,
        email_status: InviteEmailStatus,
        expires_at: Option<Timestamp>,
    ) {
        db_context
            .invites()
            .create()
            .email(EmailAddress::new(email.to_owned()).unwrap())
            .token_hash(expires_at.map(|_| email.as_bytes().to_vec()))
            .admin_id(Uuid::nil())
            .email_status(email_status)
            .expires_at(expires_at)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_invites_are_listed_newest_first_with_their_email_status_and_expiry_including_expired_ones()
     {
        let mut db_context = mock_db_context().await;
        let sent_at: Timestamp = "2026-05-01T12:00:00Z".parse().unwrap();
        let expired_at: Timestamp = "2026-05-08T12:00:00Z".parse().unwrap();
        let rejected_at = sent_at + SignedDuration::from_hours(9 * 24);
        let queued_at = rejected_at + SignedDuration::from_hours(1);
        let resent_at = queued_at + SignedDuration::from_hours(1);
        let resend_expires_at: Timestamp = "2026-05-16T12:00:00Z".parse().unwrap();
        seed(
            &mut db_context,
            "expired@household.example",
            InviteEmailStatus::Sent { sent_at },
            Some(expired_at),
        )
        .await;
        seed(
            &mut db_context,
            "resent@household.example",
            InviteEmailStatus::Queued {
                queued_at: resent_at,
            },
            Some(resend_expires_at),
        )
        .await;
        seed(
            &mut db_context,
            "refused@household.example",
            InviteEmailStatus::Rejected { rejected_at },
            None,
        )
        .await;
        seed(
            &mut db_context,
            "queued@household.example",
            InviteEmailStatus::Queued { queued_at },
            None,
        )
        .await;

        let invites = call(Dependencies { db_context })
            .await
            .unwrap()
            .invites
            .into_iter()
            .map(|x| (x.email, x.email_status, x.expires_at))
            .collect::<Vec<_>>();

        assert_eq!(
            invites,
            [
                (
                    "resent@household.example".to_owned(),
                    EmailStatus::Queued {
                        queued_at: resent_at
                    },
                    Some(resend_expires_at),
                ),
                (
                    "queued@household.example".to_owned(),
                    EmailStatus::Queued { queued_at },
                    None,
                ),
                (
                    "refused@household.example".to_owned(),
                    EmailStatus::Rejected { rejected_at },
                    None,
                ),
                (
                    "expired@household.example".to_owned(),
                    EmailStatus::Sent { sent_at },
                    Some(expired_at),
                ),
            ]
        );
    }
}
