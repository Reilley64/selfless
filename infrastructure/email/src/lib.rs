use application::attributes::SELFLESS_EMAIL_OUTCOME;
use application::attributes::SELFLESS_EMAIL_SMTP_REPLY_CODE;
use application::attributes::selfless_email_outcome;
use application::ports::SendEmail;
use application::ports::SendEmailError;
use application::spans::SELFLESS_EMAIL_SEND;
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
                otel.name = SELFLESS_EMAIL_SEND,
                otel.kind = "client",
                otel.status_code = field::Empty,
                { SELFLESS_EMAIL_OUTCOME } = field::Empty,
                { SELFLESS_EMAIL_SMTP_REPLY_CODE } = field::Empty,
            );
            let result = async {
                let message = Message::builder()
                    .from(from)
                    .to(recipient
                        .as_str()
                        .parse()
                        .map_err(SendEmailError::caused_by)?)
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
                span.record(SELFLESS_EMAIL_OUTCOME, selfless_email_outcome::SENT);
            } else {
                span.record(SELFLESS_EMAIL_OUTCOME, selfless_email_outcome::FAILED);
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

#[cfg(test)]
mod tests {
    use super::*;
    use lettre::transport::stub::AsyncStubTransport;
    use opentelemetry::KeyValue;
    use opentelemetry::trace::SpanKind;
    use opentelemetry::trace::Status;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use opentelemetry_sdk::trace::SpanData;
    use tracing::level_filters::LevelFilter;
    use tracing::subscriber::DefaultGuard;
    use tracing_subscriber::layer::SubscriberExt;

    const FROM: &str = "selfless <selfless@example.com>";
    const RECIPIENT: &str = "maya@example.com";
    const SUBJECT: &str = "You're invited to selfless";
    const BODY: &str = "Open this link to choose your name and password:\r\n\r\nhttps://selfless.example.com/invite#c2VjcmV0\r\n";

    #[tokio::test]
    async fn sends_a_plain_text_message_from_the_configured_sender() {
        let transport = AsyncStubTransport::new_ok();
        let send_email = send_email_through(transport.clone(), FROM.parse().unwrap());

        send_email
            .call(recipient(), SUBJECT.to_owned(), BODY.to_owned())
            .await
            .unwrap();

        let messages = transport.messages().await;
        let [(envelope, message)] = messages.as_slice() else {
            panic!("expected one message, got {messages:?}");
        };
        assert_eq!(
            envelope.from(),
            Some(&"selfless@example.com".parse().unwrap())
        );
        assert_eq!(envelope.to(), ["maya@example.com".parse().unwrap()]);
        let (headers, body) = message.split_once("\r\n\r\n").unwrap();
        let headers = headers.split("\r\n").collect::<Vec<_>>();
        assert!(
            headers.contains(&"From: selfless <selfless@example.com>"),
            "{headers:?}"
        );
        assert!(headers.contains(&"To: maya@example.com"), "{headers:?}");
        assert!(
            headers.contains(&"Subject: You're invited to selfless"),
            "{headers:?}"
        );
        assert!(
            headers.contains(&"Content-Type: text/plain; charset=utf-8"),
            "{headers:?}"
        );
        assert_eq!(body, BODY);
    }

    #[tokio::test]
    async fn a_sent_email_is_one_span_with_the_outcome_and_without_the_message() {
        let exporter = InMemorySpanExporter::default();
        let _tracing = trace_into(&exporter);
        let send_email = send_email_through(AsyncStubTransport::new_ok(), FROM.parse().unwrap());

        send_email
            .call(recipient(), SUBJECT.to_owned(), BODY.to_owned())
            .await
            .unwrap();

        let span = only_span(&exporter);
        assert_eq!(span.name, "selfless.email.send");
        assert_eq!(span.span_kind, SpanKind::Client);
        assert!(
            span.attributes
                .contains(&KeyValue::new("selfless.email.outcome", "sent")),
            "{span:?}"
        );
        assert_without_the_message(&span);
    }

    #[tokio::test]
    async fn a_failed_send_is_one_span_with_the_outcome_and_without_the_message() {
        let exporter = InMemorySpanExporter::default();
        let _tracing = trace_into(&exporter);
        let send_email =
            send_email_through(AsyncStubTransport::new_error(), FROM.parse().unwrap());

        let result = send_email
            .call(recipient(), SUBJECT.to_owned(), BODY.to_owned())
            .await;

        assert!(result.is_err());
        let span = only_span(&exporter);
        assert_eq!(span.name, "selfless.email.send");
        assert_eq!(span.status, Status::error(""));
        assert!(
            span.attributes
                .contains(&KeyValue::new("selfless.email.outcome", "failed")),
            "{span:?}"
        );
        assert!(
            !span
                .attributes
                .iter()
                .any(|x| x.key.as_str() == "selfless.email.smtp.reply_code"),
            "{span:?}"
        );
        assert_without_the_message(&span);
    }

    #[test]
    fn send_email_names_each_missing_smtp_setting() {
        let settings = SmtpSettings {
            host: Some("smtp.example.com".to_owned()),
            port: Some(587),
            ..SmtpSettings::default()
        };

        let error = send_email(&settings).err().unwrap();

        let missing = error
            .source()
            .and_then(|x| x.downcast_ref::<MissingSettings>())
            .unwrap();
        assert_eq!(missing.0, ["SMTP__USERNAME", "SMTP__PASSWORD", "SMTP__FROM"]);
    }

    fn recipient() -> domain::EmailAddress {
        domain::EmailAddress::new(RECIPIENT.to_owned()).unwrap()
    }

    fn trace_into(exporter: &InMemorySpanExporter) -> DefaultGuard {
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        tracing::subscriber::set_default(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        )
    }

    fn only_span(exporter: &InMemorySpanExporter) -> SpanData {
        let mut spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1, "{spans:?}");
        spans.remove(0)
    }

    fn assert_without_the_message(span: &SpanData) {
        let span = format!("{span:?}");
        for text in ["maya@example.com", "Open this link", "c2VjcmV0"] {
            assert!(!span.contains(text), "{text} is on the span: {span}");
        }
    }
}
