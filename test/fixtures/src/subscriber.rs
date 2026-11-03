use std::sync::LazyLock;
use tracing::Dispatch;
use tracing::Subscriber;
use tracing::subscriber::DefaultGuard;
use tracing::subscriber::NoSubscriber;

static SECOND_DISPATCHER_SO_TRACING_ASKS_EVERY_DISPATCHER: LazyLock<Dispatch> =
    LazyLock::new(|| Dispatch::new(NoSubscriber::default()));

pub fn set_default_subscriber<S>(subscriber: S) -> DefaultGuard
where
    S: Subscriber + Send + Sync + 'static,
{
    LazyLock::force(&SECOND_DISPATCHER_SO_TRACING_ASKS_EVERY_DISPATCHER);
    tracing::subscriber::set_default(subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Span;

    #[test]
    fn a_span_first_reached_on_a_thread_without_a_subscriber_still_reaches_the_subscriber() {
        let _tracing = set_default_subscriber(tracing_subscriber::registry());

        std::thread::spawn(span).join().unwrap();

        assert!(!span().is_disabled());
    }

    fn span() -> Span {
        tracing::info_span!("span")
    }
}
