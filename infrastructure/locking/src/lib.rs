use application::ports::AcquireLock;
use application::ports::LockGuard;
use application::ports::TryAcquireLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::OwnedMutexGuard;

#[derive(Clone, Default)]
pub struct Locks(Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>);

impl Locks {
    pub fn acquire_lock(&self) -> AcquireLock {
        let locks = self.clone();
        AcquireLock::new(move |key| {
            let (mut entry, lock) = locks.entry(key);
            Box::pin(async move {
                entry.guard = Some(lock.lock_owned().await);
                LockGuard::new(entry)
            })
        })
    }

    pub fn try_acquire_lock(&self) -> TryAcquireLock {
        let locks = self.clone();
        TryAcquireLock::new(move |key| {
            let (mut entry, lock) = locks.entry(key);
            entry.guard = lock.try_lock_owned().ok();
            Box::pin(async move { entry.guard.is_some().then(|| LockGuard::new(entry)) })
        })
    }

    fn entry(&self, key: String) -> (Entry, Arc<tokio::sync::Mutex<()>>) {
        let lock = self
            .0
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_default()
            .clone();
        let entry = Entry {
            locks: self.clone(),
            key,
            guard: None,
        };
        (entry, lock)
    }
}

struct Entry {
    locks: Locks,
    key: String,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        let mut locks = self.locks.0.lock().unwrap();
        self.guard = None;
        if locks
            .get(&self.key)
            .is_some_and(|x| Arc::strong_count(x) == 1)
        {
            locks.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_held_lock_cannot_be_tried_until_its_guard_drops() {
        let locks = Locks::default();
        let held = locks
            .acquire_lock()
            .call("library:reconcile".to_owned())
            .await;

        let while_held = locks
            .try_acquire_lock()
            .call("library:reconcile".to_owned())
            .await;
        let other_key = locks
            .try_acquire_lock()
            .call("library:confirm".to_owned())
            .await;
        drop(held);
        let after_release = locks
            .try_acquire_lock()
            .call("library:reconcile".to_owned())
            .await;

        assert!(while_held.is_none());
        assert!(other_key.is_some());
        assert!(after_release.is_some());
    }

    #[tokio::test]
    async fn a_tried_lock_makes_acquiring_wait_until_its_guard_drops() {
        let locks = Locks::default();
        let tried = locks
            .try_acquire_lock()
            .call("library:reconcile".to_owned())
            .await;
        let mut acquiring = tokio::spawn(locks.acquire_lock().call("library:reconcile".to_owned()));
        tokio::task::yield_now().await;

        let waited = !acquiring.is_finished();
        drop(tried);
        let acquired = (&mut acquiring).await;

        assert!(waited);
        assert!(acquired.is_ok());
    }
}
