use core::hint;
use core::sync::atomic::{AtomicUsize, Ordering};

/// A minimal busy-wait barrier for exactly **two threads**.
///
/// The objective is to reduce any additional delays in the measurements as much
/// as possible.
#[derive(Debug)]
pub struct NoDelayBarrier {
    // increments every time both threads meet
    arrived: AtomicUsize,
    // arrival count for the current epoch (0, 1, 2, ...)
    epoch: AtomicUsize,
}

impl NoDelayBarrier {
    /// Create a new barrier for 2 threads.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epoch: AtomicUsize::new(0),
            arrived: AtomicUsize::new(0),
        }
    }

    /// Wait until both threads have reached this point.
    /// - The *first* thread spins until the second arrives.
    /// - The *second* thread resets `arrived` and bumps `epoch` to release the first.
    pub fn wait(&self) {
        // Remember which epoch we are trying to synchronize in
        let my_epoch = self.epoch.load(Ordering::Acquire);

        // Increment arrival count; get my position (1st or 2nd)
        let arrival_count = self.arrived.fetch_add(1, Ordering::AcqRel) + 1;

        if arrival_count == 2 {
            // reset counter and advance epoch → releases the first thread
            self.arrived.store(0, Ordering::Release);
            self.epoch.fetch_add(1, Ordering::Release);
        } else {
            // spin until epoch changes (second thread has arrived)
            while self.epoch.load(Ordering::Acquire) == my_epoch {
                hint::spin_loop();
            }
        }
    }

    /// Force-release the barrier for this round only.
    /// Any threads currently stuck in `wait()` will resume, and
    /// the barrier state is reset for the next round.
    pub fn unblock(&self) {
        // Reset arrivals so the next round starts fresh
        self.arrived.store(0, Ordering::Release);
        // Bump epoch to release all spinners
        self.epoch.fetch_add(1, Ordering::Release);
    }
}

impl Default for NoDelayBarrier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn two_threads_meet_multiple_times() {
        let barrier = Arc::new(NoDelayBarrier::new());
        let rounds = 10;

        let b1 = barrier.clone();
        let t1 = thread::spawn(move || {
            for _ in 0..rounds {
                b1.wait();
            }
        });

        let b2 = barrier;
        let t2 = thread::spawn(move || {
            for _ in 0..rounds {
                b2.wait();
            }
        });

        // If they deadlock, this join will hang forever.
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[test]
    fn unblock_releases_waiter() {
        let barrier = Arc::new(NoDelayBarrier::new());

        let b1 = barrier.clone();
        let t1 = thread::spawn(move || {
            // This thread will wait, expecting to be released by unblock.
            b1.wait();
            "released"
        });

        // Give t1 a chance to enter wait()
        thread::sleep(Duration::from_millis(50));

        barrier.unblock();

        let start = Instant::now();
        let res = t1.join().unwrap();
        assert_eq!(res, "released");
        assert!(
            start.elapsed() < Duration::from_millis(10),
            "waiter was not released promptly"
        );
    }
}
