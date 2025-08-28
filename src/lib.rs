#![deny(
    clippy::all,
    clippy::cargo,
    clippy::nursery,
    clippy::must_use_candidate,
    clippy::perf
    // clippy::restriction,
    // clippy::pedantic
)]
// now allow a few rules which are denied by the above statement
// --> they are ridiculous and not necessary
#![allow(
    clippy::suboptimal_flops,
    clippy::redundant_pub_crate,
    clippy::fallible_impl_from
)]
#![deny(missing_debug_implementations)]

pub mod channel;
pub mod condvar;
pub mod sleeper_thread;
pub mod synchronization;

use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum WakeupReason {
    Timeout,
    Interrupted { wake_call_instant: Instant },
}

#[derive(Debug)]
pub struct WakeupContext {
    pub reason: WakeupReason,
    pub expected_duration: Duration,
    pub actual_duration: Duration,
    pub delay: Duration,
}

/// A sleeper that puts the executing thread context into an interruptible
/// sleep.
pub trait Sleeper {
    /// Puts the thread into sleep that is interruptible.
    fn sleep_interruptible(&self, sleep_duration: Duration) -> WakeupReason;
}

/// A waker for a [`Sleeper`].
pub trait Waker {
    /// Wakes the corresponding [`Sleeper`].
    ///
    /// The implementation is supposed to send the current [`Instant`] to the
    /// [`Sleeper`] so that it can properly construct the [`WakeupContext`].
    ///
    /// This function waits for the [`Sleeper`] to acknowledge the wake-up
    /// call. The main motivation of this property is to facilitate
    /// unit-testing and prevent race conditions. This synchronization should
    /// add as little delay as possible by using a [`NoDelayBarrier`].
    ///
    /// [`NoDelayBarrier`]: crate::synchronization::NoDelayBarrier
    fn wake(&self);
}

#[derive(Debug)]
pub struct Measurement {
    pub wakeup_context: WakeupContext,
}

#[derive(Debug)]
pub struct Measurements {
    pub interrupted: Vec<Measurement>,
    pub timeouted: Vec<Measurement>,
    pub rounds: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synchronization::NoDelayBarrier;
    use assert2::check;
    use std::sync::Arc;
    use std::sync::mpsc::sync_channel;
    use std::thread::sleep;

    const TIMESLICE: Duration = Duration::from_millis(50);

    // basic functionality test for sleeper and waker
    fn test_sleeper(sleeper: impl Sleeper + Send + 'static, waker: impl Waker + 'static) {
        let (sender, receiver) = sync_channel::<WakeupContext>(1);
        let sleep_barrier = Arc::new(NoDelayBarrier::new());

        let thread =
            sleeper_thread::SleeperThread::spawn(sleep_barrier.clone(), sleeper, TIMESLICE, sender);

        eprintln!("test case 1/3");
        {
            sleep_barrier.wait();
            let wakeup_context = receiver.recv().unwrap();
            check!(wakeup_context.reason == WakeupReason::Timeout);
            check!(wakeup_context.actual_duration >= TIMESLICE);
        }
        eprintln!("test case 2/3");
        {
            sleep_barrier.wait();
            sleep(Duration::from_millis(1));
            waker.wake();
            let wakeup_context = receiver.recv().unwrap();
            assert2::assert!(matches!(
                wakeup_context.reason,
                WakeupReason::Interrupted { .. }
            ));
            check!(wakeup_context.actual_duration <= TIMESLICE / 2);
        }
        eprintln!("test case 3/3");
        {
            sleep_barrier.wait();
            let wakeup_context = receiver.recv().unwrap();
            check!(wakeup_context.reason == WakeupReason::Timeout);
            check!(wakeup_context.actual_duration >= TIMESLICE);
        }

        drop(thread);
    }

    // TODO also park/unpark waker
    #[test]
    fn test_channel_sleeper() {
        let (sleeper, waker) = channel::new_pair();
        test_sleeper(sleeper, waker);
    }

    #[test]
    fn test_condvar_sleeper() {
        let (sleeper, waker) = condvar::new_pair();
        test_sleeper(sleeper, waker);
    }
}
