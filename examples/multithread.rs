#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate futures;
extern crate tacho;
extern crate tokio_core;
extern crate tokio_timer;

use futures::{BoxFuture, Future, Stream};
use futures::sync::oneshot;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use tacho::Timing;
use tokio_core::reactor::Core;
use tokio_timer::Timer;

// A performance test for an asynchronous Scope reporter with timers, counters, and
// gauges.
fn main() {
    drop(pretty_env_logger::init());

    let (metrics, report) = tacho::new();

    let (work_done_tx, work_done_rx) = oneshot::channel();
    let reporter = {
        let work_done_rx = work_done_rx.map_err(|_| ());
        let interval = Duration::from_secs(2);
        reporter(interval, work_done_rx, report)
    };
    {
        let metrics = metrics.clone().labeled("role".into(), "worker".into());
        let mut total_time_ms = metrics.gauge("total_time_ms".into());
        let mut loop_counter = metrics.counter("loop_iters_count".into());
        let mut loop_gauge = metrics.gauge("loop_iters_curr".into());
        let mut loop_time_us = metrics.stat("loop_time_us".into());

        let spawn_start = Timing::start();
        thread::spawn(move || {
            for i in 0..100_000_000 {
                let loop_start = Timing::start();
                loop_counter.incr(1);
                loop_gauge.set(i);
                loop_time_us.add(loop_start.elapsed_us());
            }
            total_time_ms.set(spawn_start.elapsed_ms());
            work_done_tx.send(()).expect("could not send");
        });
    }

    let heartbeat = {
        let metrics = metrics.labeled("role".into(), "heartbeat".into());
        let mut heartbeats = metrics.gauge("heartbeats".into());
        Timer::default()
            .interval(Duration::from_secs(1))
            .fold(0, move |i, _| {
                trace!("heartbeat");
                heartbeats.set(i);
                Ok(i + 1)
            })
            .map(|_| {})
            .map_err(|_| {})
    };

    let mut core = Core::new().expect("Failed to create core");
    core.handle().spawn(heartbeat);
    core.run(reporter).expect("failed to run reporter");
}

/// Prints a report every `interval` and when the `done` is satisfied.
fn reporter<D>(interval: Duration, done: D, reporter: tacho::Reporter) -> BoxFuture<(), ()>
    where D: Future<Item = (), Error = ()> + Send + 'static
{
    let periodic = {
        let mut reporter = reporter.clone();
        Timer::default()
            .interval(interval)
            .map_err(|_| {})
            .for_each(move |_| {
                          print_report(&reporter.take());
                          Ok(())
                      })
    };
    let done = done.map(move |_| {
        print_report(&reporter.peek());
    });
    periodic.select(done).map(|_| {}).map_err(|_| {}).boxed()
}

fn print_report<R: tacho::Report>(report: &R) {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let _ = stdout.write(b"\n");
    let _ = stdout.write_all(tacho::prometheus::format(report).as_bytes());
}
