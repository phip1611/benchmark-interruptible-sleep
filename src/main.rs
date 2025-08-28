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

use assert2::check;
use benchmark_interruptible_sleep::synchronization::NoDelayBarrier;
use benchmark_interruptible_sleep::{Measurement, Measurements, Sleeper, Waker, WakeupContext, WakeupReason, channel, sleeper_thread, condvar};
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Optimized sleep that won't have any delay close to the target timeout due to
/// busy waiting.
fn sleep_optimized(duration: Duration) {
    const SAFE_SLEEP: Duration = Duration::from_millis(2);
    let begin = Instant::now();
    if duration > SAFE_SLEEP {
        sleep(duration - SAFE_SLEEP);
    }

    // busy waiting to not lose a timeout
    while begin.elapsed() <= duration {}
}

/// Runs many cycles of [`Sleeper::sleep_interruptible`] in a thread: some
/// timeout normally and some get interrupted.
///
/// Collects the effective delay between the [`Waker::wake`] call and the actual
/// awakening. While doing so, this thread is synchronized with a corresponding
/// [`SleeperThread`].
///
/// # Arguments
/// - `timeslice`: The normal time slice for that we put threads into sleep. Reducing the selected
///   time frame increases the impact of OS scheduling and additional runtime overhead.
fn test_runs(
    rounds: usize,
    sleeper: impl Sleeper + Send + 'static,
    waker: impl Waker + 'static,
    timeslice: Duration,
) -> Measurements {
    let waker = Arc::new(waker);
    let mut timeouted_results = Vec::<Measurement>::with_capacity(1_000_000);
    let mut interrupted_results = Vec::<Measurement>::with_capacity(1_000_000);
    // We only transport one item at a time. Threads are synchronized.
    let (sender, receiver) = sync_channel::<WakeupContext>(1);
    let sleep_barrier = Arc::new(NoDelayBarrier::new());
    let _thread =
        sleeper_thread::SleeperThread::spawn(sleep_barrier.clone(), sleeper, timeslice, sender);

    loop {
        if interrupted_results.len() + timeouted_results.len() >= rounds {
            break;
        }

        let do_interrupt = fastrand::bool();
        let sleep_duration = if do_interrupt {
            let max_us = timeslice.as_micros() as usize;
            let max_us = max_us * 95 / 100;
            let rand_us = fastrand::usize(0..=max_us);
            Duration::from_micros(rand_us as u64)
        } else {
            timeslice
        };

        // Wait for the other thread to start a new cycle.
        sleep_barrier.wait();

        sleep_optimized(sleep_duration);
        if do_interrupt {
            waker.wake();
        }

        let wakeup_context = receiver.recv().unwrap();

        if do_interrupt {
            check!(matches!(
                wakeup_context.reason,
                WakeupReason::Interrupted { .. }
            ));
            interrupted_results.push(Measurement { wakeup_context });
        } else {
            check!(wakeup_context.reason == WakeupReason::Timeout);
            timeouted_results.push(Measurement { wakeup_context });
        }
    }

    let rounds = interrupted_results.len() + timeouted_results.len();
    Measurements {
        interrupted: interrupted_results,
        timeouted: timeouted_results,
        rounds,
    }
}

fn calc_mean(data: &[Measurement]) -> Duration {
    let len = data.len();
    if len == 0 {
        Duration::ZERO
    } else {
        let sum = data
            .iter()
            .map(|m| m.wakeup_context.delay)
            .sum::<Duration>();
        sum / (len as u32)
    }
}

fn print_analysis(measurements: Measurements) {
    let interrupted_delay_mean = calc_mean(&measurements.interrupted);
    let timeouted_delay_mean = calc_mean(&measurements.timeouted);

    println!("Rounds        (#): {}", measurements.rounds);
    println!("  interrupted (#): {}", measurements.interrupted.len());
    println!(
        "  |- mean delay  : {:>5} µs",
        interrupted_delay_mean.as_micros()
    );
    println!("  timeouted   (#): {}", measurements.timeouted.len());
    println!(
        "  |- mean delay  : {:>5} µs",
        timeouted_delay_mean.as_micros()
    );
}

fn main() {
    let rounds = 100;
    let timeslices_ms = [2, 5, 10, 25, 50, 100];

    for timeslice in timeslices_ms {
        // Sleeper #1: mutex + condvar
        {
            println!("TEST RUN: Condvar Sleeper, timeslice={:>3}ms, rounds={rounds}", timeslice);
            let (sleeper, waker) = condvar::new_pair();
            let measurements = test_runs(rounds, sleeper, waker, Duration::from_millis(timeslice));
            print_analysis(measurements);
        }

        println!();
        // Sleeper #2: channel
        {
            println!(
                "TEST RUN: Channel Sleeper, timeslice={:>3}ms, rounds={rounds}",
                timeslice
            );
            let (sleeper, waker) = channel::new_pair();
            let measurements = test_runs(rounds, sleeper, waker, Duration::from_millis(timeslice));
            print_analysis(measurements);
        }

        println!();
    }
}
