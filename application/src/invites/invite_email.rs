use crate::ports::PortFuture;
use crate::ports::SendEmail;
use crate::ports::SendEmailError;

pub(crate) fn send(
    send_email: &SendEmail,
    recipient: String,
    origin: &str,
    token: &str,
) -> PortFuture<Result<(), SendEmailError>> {
    send_email.call(
        recipient,
        "You're invited to selfless".to_owned(),
        format!(
            "You're invited to selfless.\n\nOpen this link to choose your name and password:\n{origin}/invite#{token}\n\nThe link works once and expires in 7 days.\n"
        ),
    )
}
