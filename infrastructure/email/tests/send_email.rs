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
    let send_email = email::send_email_through(transport.clone(), FROM.parse().unwrap());

    send_email
        .call(RECIPIENT.to_owned(), SUBJECT.to_owned(), BODY.to_owned())
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
    let send_email = email::send_email_through(AsyncStubTransport::new_ok(), FROM.parse().unwrap());

    send_email
        .call(RECIPIENT.to_owned(), SUBJECT.to_owned(), BODY.to_owned())
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
        email::send_email_through(AsyncStubTransport::new_error(), FROM.parse().unwrap());

    let result = send_email
        .call(RECIPIENT.to_owned(), SUBJECT.to_owned(), BODY.to_owned())
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
