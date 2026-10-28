//! Work done a moment after it was last asked for: a draft is written a second and a half after the
//! typing stops, the page's state a second after it stops moving. Each piece of work has a key;
//! asking again for a key moves its time on. One thread does the work of all the keys, in time order.
//!
//! Plain values: this file knows nothing of the page.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

struct Due<K> {
    at: HashMap<K, Instant>,
    /// The owner is gone: the thread ends.
    closed: bool,
}

struct Shared<K> {
    due: Mutex<Due<K>>,
    changed: Condvar,
}

impl<K> Shared<K> {
    fn lock(&self) -> MutexGuard<'_, Due<K>> {
        self.due.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Keyed work, each done once at its time unless it is asked for again or called off first.
pub struct Later<K> {
    shared: Arc<Shared<K>>,
}

impl<K: Eq + Hash + Clone + Send + 'static> Later<K> {
    /// Work that `run` does, on a thread named `name`.
    pub fn new(name: &str, run: impl Fn(K) + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            due: Mutex::new(Due {
                at: HashMap::new(),
                closed: false,
            }),
            changed: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        // Without the thread nothing is done later; what has to last is written when the program
        // ends all the same.
        let _ = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || work(&worker, &run));
        Self { shared }
    }

    /// The work of `key`, `delay` from now (and not at the time it had).
    pub fn after(&self, key: K, delay: Duration) {
        self.shared.lock().at.insert(key, Instant::now() + delay);
        self.shared.changed.notify_one();
    }

    /// The work of `key` is not done.
    pub fn cancel(&self, key: &K) {
        self.shared.lock().at.remove(key);
        self.shared.changed.notify_one();
    }
}

impl<K> Drop for Later<K> {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.changed.notify_one();
    }
}

fn work<K: Eq + Hash + Clone>(shared: &Shared<K>, run: &impl Fn(K)) {
    let mut due = shared.lock();
    loop {
        if due.closed {
            return;
        }
        let next = due
            .at
            .iter()
            .min_by_key(|(_, at)| **at)
            .map(|(key, at)| (key.clone(), *at));
        due = match next {
            None => shared
                .changed
                .wait(due)
                .unwrap_or_else(PoisonError::into_inner),
            Some((key, at)) if at <= Instant::now() => {
                due.at.remove(&key);
                // The work is done with the keys free, so that it may ask for more.
                drop(due);
                run(key);
                shared.lock()
            }
            Some((_, at)) => {
                shared
                    .changed
                    .wait_timeout(due, at.saturating_duration_since(Instant::now()))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn work_is_done_once_after_its_last_asking_and_not_when_called_off() {
        let (done, heard) = mpsc::channel();
        let later = Later::new("test", move |key: &'static str| {
            let _ = done.send((key, Instant::now()));
        });
        let asked = Instant::now();
        later.after("a", Duration::from_millis(50));
        later.after("b", Duration::from_millis(20));
        later.after("a", Duration::from_millis(80));
        later.after("c", Duration::from_millis(30));
        later.cancel(&"c");
        let first = heard.recv_timeout(Duration::from_secs(2)).unwrap();
        let second = heard.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!((first.0, second.0), ("b", "a"));
        assert!(second.1 - asked >= Duration::from_millis(80));
        assert!(heard.recv_timeout(Duration::from_millis(150)).is_err());
    }
}
