use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::Unit;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct FakeClock(Arc<Mutex<Timestamp>>);

impl FakeClock {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(
            Timestamp::now().round(Unit::Second).unwrap(),
        )))
    }

    pub fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }

    pub fn advance(&self, duration: SignedDuration) {
        *self.0.lock().unwrap() += duration;
    }

    pub fn reader(&self) -> impl Fn() -> Timestamp + Send + Sync + 'static {
        let now = self.0.clone();
        move || *now.lock().unwrap()
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}
