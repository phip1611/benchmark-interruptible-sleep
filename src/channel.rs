//! Implements [`Sleeper`] and [`Waker`] using a Rust channel from
//! the std library.

use crate::synchronization::NoDelayBarrier;
use crate::{Sleeper, Waker, WakeupReason};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct ChannelSleeper {
    receiver: Receiver<Instant>,
    // Barrier to synchronize sleep_interruptible() and wake()
    synchronization_point: Arc<NoDelayBarrier>,
}

#[derive(Debug)]
pub struct ChannelWaker {
    sender: SyncSender<Instant>,
    // Helper to synchronize sleep_interruptible() and wake()
    synchronization_point: Arc<NoDelayBarrier>,
}

#[must_use]
pub fn new_pair() -> (ChannelSleeper, ChannelWaker) {
    let (sender, receiver) = sync_channel(1);
    let synchronization_point = Arc::new(NoDelayBarrier::new());
    let sleeper = ChannelSleeper {
        receiver,
        synchronization_point: synchronization_point.clone(),
    };
    let waker = ChannelWaker {
        sender,
        synchronization_point,
    };

    (sleeper, waker)
}

impl Sleeper for ChannelSleeper {
    fn sleep_interruptible(&self, sleep_duration: Duration) -> WakeupReason {
        let res = self.receiver.recv_timeout(sleep_duration);
        match res {
            Ok(instant) => {
                let reason = WakeupReason::Interrupted {
                    wake_call_instant: instant,
                };

                // Unblock Waker::wake()
                self.synchronization_point.wait();

                reason
            }
            Err(RecvTimeoutError::Timeout) => {
                // TODO does that ever happen?
                // Unblock in case we were awakened at a time when also the
                // timeout was due.
                // self.synchronization_point.unblock();
                WakeupReason::Timeout
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("Channel disconnected");
            }
        }
    }
}

impl Waker for ChannelWaker {
    fn wake(&self) {
        self.sender.send(Instant::now()).unwrap();
        // Wait for sleep() to be interrupted.
        self.synchronization_point.wait();
    }
}
