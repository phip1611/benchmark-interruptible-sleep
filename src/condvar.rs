//! Implements [`Sleeper`] and [`Waker`] using a Mutex and a Condvar from
//! the std library.

use crate::synchronization::NoDelayBarrier;
use crate::{Sleeper, Waker, WakeupReason};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const SLEEP_WAS_INTERRUPTED: bool = true;
const SLEEP_NORMAL: bool = false;

#[derive(Debug)]
struct SleepWakeContext {
    sleep_state: bool,
    wake_call_instant: Option<Instant>,
}

impl Default for SleepWakeContext {
    fn default() -> Self {
        Self {
            sleep_state: SLEEP_NORMAL,
            wake_call_instant: None,
        }
    }
}

#[derive(Debug)]
pub struct CondvarSleeper {
    shared_state: Arc<(Condvar, Mutex<SleepWakeContext>)>,
    // Helper to synchronize sleep_interruptible() and wake()
    synchronization_point: Arc<NoDelayBarrier>,
}

#[derive(Debug)]
pub struct CondvarWaker {
    shared_state: Arc<(Condvar, Mutex<SleepWakeContext>)>,
    // Helper to synchronize sleep_interruptible() and wake()
    synchronization_point: Arc<NoDelayBarrier>,
}

#[must_use]
pub fn new_pair() -> (CondvarSleeper, CondvarWaker) {
    let mutex = Mutex::new(SleepWakeContext::default());
    let condvar = Condvar::new();
    let shared_state = Arc::new((condvar, mutex));
    let synchronization_point = Arc::new(NoDelayBarrier::new());

    let sleeper = CondvarSleeper {
        shared_state: shared_state.clone(),
        synchronization_point: synchronization_point.clone(),
    };
    let waker = CondvarWaker {
        shared_state,
        synchronization_point,
    };

    (sleeper, waker)
}

impl Sleeper for CondvarSleeper {
    #[allow(clippy::significant_drop_tightening)]
    fn sleep_interruptible(&self, sleep_duration: Duration) -> WakeupReason {
        let mut guard = self.shared_state.1.lock().unwrap();

        loop {
            let (guard_, res) = self
                .shared_state
                .0
                .wait_timeout(guard, sleep_duration)
                .unwrap();
            guard = guard_;

            if res.timed_out() {
                break WakeupReason::Timeout;
            }

            if guard.sleep_state == SLEEP_NORMAL {
                panic!("We woke up too early");
            }

            if guard.sleep_state == SLEEP_WAS_INTERRUPTED {
                let wakeup_reason = WakeupReason::Interrupted {
                    wake_call_instant: guard
                        .wake_call_instant
                        .take()
                        .expect("should have been set by wake()"),
                };
                // Reset
                guard.sleep_state = SLEEP_NORMAL;

                // Unblock Waker::wake()
                self.synchronization_point.wait();

                break wakeup_reason;
            } else {
                // TODO does that ever happen?
                // Unblock in case we were awakened at a time when also the
                // timeout was due.
                // self.synchronization_point.unblock();
            }
        }
    }
}

impl Waker for CondvarWaker {
    fn wake(&self) {
        let mut guard = self.shared_state.1.lock().unwrap();
        guard.sleep_state = SLEEP_WAS_INTERRUPTED;
        guard.wake_call_instant = Some(Instant::now());
        self.shared_state.0.notify_one();
        drop(guard);

        // Wait for Sleeper to ACK
        self.synchronization_point.wait();
    }
}
