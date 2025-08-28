# Benchmarking Interruptible Sleep: Rust Channels vs Rust Condvar

Sometimes you want to put threads into sleep but have fine-grained control to wake them up ASAP if necessary.
[`thread::sleep()`](https://doc.rust-lang.org/std/thread/fn.sleep.html) is not interruptible but
[`Condvar::wait_timeout`](https://doc.rust-lang.org/std/sync/struct.Condvar.html#method.wait_timeout)
and [`Receiver::recv_timeout`](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout)
are.

The Rust documentation there says:

>  This method should not be used for precise timing due to anomalies such as preemption or platform differences that might not cause the maximum amount of time waited to be precisely dur.

But what does that mean? Sometimes good is good enough and we have two variants to achieve the same thing.

For that, I've created this Rust benchmark. It runs benchmarks for different `Sleeper` and `Waker` implementations
with different default sleep timeslices. It measures the effective `wake()` overhead, i.e., how fast is the thread
back in running state when it was interrupted.

## Benchmark Results

⚠️ Please note that the data may change depending on the platform (x86_64, ARM, ...), your hardware, the Operating System (Microsoft Windows, MacOS, $ Linux Distribution, ...),
and rustc version, which effects the compiler and the standard library.

### Test System

- NixOS 25.05, Kernel 6.12.41
- AMD Ryzen 7 7840U
- rustc 1.89
- `cargo run --release` with maximum settings (see Cargo configuration)

### TL;DR

- The performance of both is roughly equal

### Results

Measurements were taken with `1000` rounds per setting.

<!-- TODO outdated -->

| Timeslice (ms) | Test                   | Condvar Sleeper (µs) | Channel Sleeper (µs) |
|----------------|------------------------|----------------------|----------------------|
| 2              | Interrupted Mean Delay | 248                  | 251                  |
|                | Timeouted Mean Delay   | 59                   | 56                   |
| 5              | Interrupted Mean Delay | 624                  | 615                  |
|                | Timeouted Mean Delay   | 68                   | 69                   |
| 10             | Interrupted Mean Delay | 1248                 | 1262                 |
|                | Timeouted Mean Delay   | 81                   | 69                   |
| 50             | Interrupted Mean Delay | 6245                 | 6226                 |
|                | Timeouted Mean Delay   | 82                   | 79                   |
| 100            | Interrupted Mean Delay | 12878                | 12232                |
|                | Timeouted Mean Delay   | 83                   | 83                   |
|----------------|------------------------| -----------------    | -----------------    |


