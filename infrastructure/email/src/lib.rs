use application::attributes::SELFLESS_EMAIL_OUTCOME;
use application::attributes::SELFLESS_EMAIL_SMTP_REPLY_CODE;
use application::ports::SendEmail;
use application::ports::SendEmailError;
use lettre::AsyncSmtpTransport;
use lettre::AsyncTransport;
use lettre::Message;
use lettre::Tokio1Executor;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::response::Code;
use serde::Deserialize;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use tracing::Instrument;
use tracing::Span;
use tracing::field;

const IMPLICIT_TLS_PORT: u16 = 465;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct SmtpSettings {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Option<Mailbox>,
}

#[ohno::error]
#[display("failed to set up email")]
pub struct SetUpError;

pub fn send_email(settings: &SmtpSettings) -> Result<SendEmail, SetUpError> {
    let (Some(host), Some(port), Some(username), Some(password), Some(from)) = (
        &settings.host,
        settings.port,
        &settings.username,
        &settings.password,
        &settings.from,
    ) else {
        return Err(SetUpError::caused_by(MissingSettings(
            [
                ("SMTP__HOST", settings.host.is_none()),
                ("SMTP__PORT", settings.port.is_none()),
                ("SMTP__USERNAME", settings.username.is_none()),
                ("SMTP__PASSWORD", settings.password.is_none()),
                ("SMTP__FROM", settings.from.is_none()),
            ]
            .into_iter()
            .filter_map(|(name, missing)| missing.then_some(name))
            .collect(),
        )));
    };
    let transport = if port == IMPLICIT_TLS_PORT {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    }
    .map_err(SetUpError::caused_by)?
    .port(port)
    .credentials(Credentials::new(username.clone(), password.clone()))
    .build();
    Ok(send_email_through(transport, from.clone()))
}

pub fn send_email_through<T>(transport: T, from: Mailbox) -> SendEmail
where
    T: AsyncTransport + Send + Sync + 'static,
    T::Error: Error + Send + Sync + 'static,
{
    let transport = Arc::new(transport);
    SendEmail::new(move |recipient, subject, body| {
        let (transport, from) = (transport.clone(), from.clone());
        Box::pin(async move {
            let span = tracing::info_span!(
                "email_send",
                otel.name = "selfless.email.send",
                otel.kind = "client",
                otel.status_code = field::Empty,
                { SELFLESS_EMAIL_OUTCOME } = field::Empty,
                { SELFLESS_EMAIL_SMTP_REPLY_CODE } = field::Empty,
            );
            let result = async {
                let message = Message::builder()
                    .from(from)
                    .to(recipient.parse().map_err(SendEmailError::caused_by)?)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(body)
                    .map_err(SendEmailError::caused_by)?;
                transport.send(message).await.map_err(|error| {
                    match (&error as &dyn Error)
                        .downcast_ref::<smtp::Error>()
                        .and_then(smtp::Error::status)
                    {
                        Some(code) => {
                            Span::current()
                                .record(SELFLESS_EMAIL_SMTP_REPLY_CODE, i64::from(u16::from(code)));
                            SendEmailError::caused_by(SmtpReply(code))
                        }
                        None => SendEmailError::caused_by(error),
                    }
                })?;
                Ok(())
            }
            .instrument(span.clone())
            .await;
            if result.is_ok() {
                span.record(SELFLESS_EMAIL_OUTCOME, "sent");
            } else {
                span.record(SELFLESS_EMAIL_OUTCOME, "failed");
                span.record("otel.status_code", "ERROR");
            }
            result
        })
    })
}

#[derive(Debug)]
struct SmtpReply(Code);

impl fmt::Display for SmtpReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the SMTP server replied {}", self.0)
    }
}

impl Error for SmtpReply {}

#[derive(Debug)]
struct MissingSettings(Vec<&'static str>);

impl fmt::Display for MissingSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing settings: {}", self.0.join(", "))
    }
}

impl Error for MissingSettings {}
