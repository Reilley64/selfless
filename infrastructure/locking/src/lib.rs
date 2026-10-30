use application::ports::AcquireLock;
use application::ports::LockGuard;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::OwnedMutexGuard;

type Locks = Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

pub fn acquire_lock() -> AcquireLock {
    let locks = Locks::default();
    AcquireLock::new(move |key| {
        let locks = locks.clone();
        Box::pin(async move {
            let lock = locks
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_default()
                .clone();
            let mut entry = Entry {
                locks,
                key,
                guard: None,
            };
            entry.guard = Some(lock.lock_owned().await);
            LockGuard::new(entry)
        })
    })
}

struct Entry {
    locks: Locks,
    key: String,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        let mut locks = self.locks.lock().unwrap();
        self.guard = None;
        if locks
            .get(&self.key)
            .is_some_and(|x| Arc::strong_count(x) == 1)
        {
            locks.remove(&self.key);
        }
    }
}
