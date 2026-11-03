use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;

type Sent<E> = Pin<Box<dyn Future<Output = Result<(), E>> + Send>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentEmail {
    pub recipient: String,
    pub subject: String,
    pub body: String,
}

impl SentEmail {
    pub fn invite_token(&self, origin: &str) -> String {
        self.body
            .split(&format!("{origin}/invite#"))
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }
}

#[derive(Clone, Default)]
pub struct FakeMailer {
    sent: Arc<Mutex<Vec<SentEmail>>>,
}

impl FakeMailer {
    pub fn sent(&self) -> Vec<SentEmail> {
        self.sent.lock().unwrap().clone()
    }

    pub fn sender<R: ToString, E: Send + 'static>(
        &self,
    ) -> impl Fn(R, String, String) -> Sent<E> + Send + Sync + 'static {
        let sent = self.sent.clone();
        move |recipient, subject, body| {
            sent.lock().unwrap().push(SentEmail {
                recipient: recipient.to_string(),
                subject,
                body,
            });
            Box::pin(async { Ok(()) })
        }
    }
}
