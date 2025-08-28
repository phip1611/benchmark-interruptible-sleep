//! Module for sleeper control. See [`SleeperThread`].

use crate::synchronization::NoDelayBarrier;
use crate::{Sleeper, WakeupContext, WakeupReason};
use assert2::check;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Barrier};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SHOULD_EXIT: bool = true;
const SHOULD_CONTINUE: bool = false;

/// Handle to a thread that continuously sleeps on a [`Sleeper`] and measures
/// the effective wakeup times.
///
/// The results are send through a channel for further analysis.
///
/// The thread is supposed to be used by the controlling thread, doing the
/// actual interruptions and collecting measurements.
#[derive(Debug)]
pub struct SleeperThread {
    thread_task: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    sleep_barrier: Arc<NoDelayBarrier>,
}

impl SleeperThread {
    fn thread_fn<S: Sleeper>(
        sleeper: S,
        sleep_barrier: Arc<NoDelayBarrier>,
        thread_task: Arc<AtomicBool>,
        default_sleep_duration: Duration,
        sender: SyncSender<WakeupContext>,
        thread_startup_barrier: Arc<Barrier>,
    ) -> impl FnOnce() {
        move || {
            // Notify caller that thread has started.
            thread_startup_barrier.wait();
            loop {
                // Wait for the control thread to be ready for the next
                // measurement cycle.
                sleep_barrier.wait();

                // Exit thread gracefully if necessary.
                if thread_task.load(Ordering::SeqCst) == SHOULD_EXIT {
                    break;
                }

                let begin = Instant::now();
                let wakeup_reason = sleeper.sleep_interruptible(default_sleep_duration);
                let actual_sleep_duration_with_overhead = begin.elapsed();

                // Exit directly, ignoring the sender.
                if thread_task.load(Ordering::SeqCst) == SHOULD_EXIT {
                    break;
                }

                // Determine the ideal/perfect sleep duration.
                let actual_expected_sleep_duration =
                    if let WakeupReason::Interrupted { wake_call_instant } = wakeup_reason {
                        check!(wake_call_instant >= begin);
                        wake_call_instant - begin
                    } else {
                        default_sleep_duration
                    };

                // The delay between `sleep()` and `wake()`.
                check!(actual_expected_sleep_duration <= actual_sleep_duration_with_overhead);
                let delay = actual_sleep_duration_with_overhead - actual_expected_sleep_duration;

                let wakeup_context = WakeupContext {
                    reason: wakeup_reason,
                    expected_duration: actual_expected_sleep_duration,
                    actual_duration: actual_sleep_duration_with_overhead,
                    delay,
                };

                // Send the result to the control thread, allowing analysis.
                sender.send(wakeup_context).unwrap();
            }
        }
    }

    /// Spawns a new thread.
    ///
    /// Waits for the thread to start. Afterward, the thread will wait for
    /// sleep() events, synchronized via  the shared `sleep_barrier` of type
    /// [`NoDelayBarrier`].
    pub fn spawn<S: Sleeper + Send + 'static>(
        sleep_barrier: Arc<NoDelayBarrier>,
        sleeper: S,
        default_sleep_duration: Duration,
        sender: SyncSender<WakeupContext>,
    ) -> Self {
        let thread_task = Arc::new(AtomicBool::new(SHOULD_CONTINUE));
        let thread_startup_barrier = Arc::new(Barrier::new(2));
        let handle = {
            let thread_task = thread_task.clone();
            let sleep_barrier = sleep_barrier.clone();
            thread::spawn(Self::thread_fn(
                sleeper,
                sleep_barrier,
                thread_task,
                default_sleep_duration,
                sender,
                thread_startup_barrier.clone(),
            ))
        };

        // Wait for thread to start up.
        thread_startup_barrier.wait();

        Self {
            handle: Some(handle),
            thread_task,
            sleep_barrier,
        }
    }
}

impl Drop for SleeperThread {
    fn drop(&mut self) {
        // Tell thread to exit on it's next iteration.
        self.thread_task.store(SHOULD_EXIT, Ordering::SeqCst);

        // unblock thread from "waiting for work"
        self.sleep_barrier.unblock();

        // terminate thread handle
        let handle = self.handle.take().expect("should still have thread handle");
        handle.join().expect("should gracefully exit thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread::sleep;
    use crate::Waker;

    struct Dummy;
    impl Waker for Dummy {
        fn wake(&self) {}
    }

    impl Sleeper for Dummy {
        fn sleep_interruptible(&self, _sleep_duration: Duration) -> WakeupReason {
            sleep(Duration::from_millis(50));
            WakeupReason::Timeout
        }
    }

    #[test]
    fn test_thread_lifecycle() {
        let sleeper_barrier = Arc::new(NoDelayBarrier::new());
        let (sender, _receiver) = mpsc::sync_channel(1);
        let thread = SleeperThread::spawn(sleeper_barrier, Dummy, Duration::ZERO, sender);

        // Test succeeds if this does not get stuck.
        drop(thread);
    }
}
