#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate futures;
extern crate tacho;
extern crate tokio_core;
extern crate tokio_timer;

use futures::{Future, Stream};
use futures::sync::BiLock;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use tacho::{Tacho, Timing};
use tokio_core::reactor::Core;
use tokio_timer::Timer;

// A performance test for an asynchronous Metrics reporter with timers, counters, and
// gauges.
fn main() {
    drop(pretty_env_logger::init());

    let Tacho { metrics, aggregator, report } = Tacho::default();

    let work_thread = {
        let metrics = metrics.clone().labeled("role".into(), "worker".into());
        let total_time = metrics.scope().timing_ms("time_us".into());
        let loop_counter = metrics.scope().counter("iters_count".into());
        let loop_gauge = metrics.scope().gauge("iters_curr".into());
        let loop_time = metrics.scope().timing_ms("time_ms".into());

        let spawn_start = Timing::start();
        thread::spawn(move || {
            for i in 0..100_000_000 {
                let loop_start = Timing::start();
                let mut r = metrics.recorder();
                r.incr(&loop_counter, 1);
                r.set(&loop_gauge, i);
                r.add(&loop_time, loop_start.elapsed_us());
            }
            {
                let mut r = metrics.recorder();
                r.add(&total_time, spawn_start.elapsed_ms());
            }
        })
    };

    let heartbeat = {
        let metrics = metrics.clone().labeled("role".into(), "heartbeat".into());
        let heartbeats = metrics.scope().gauge("heartbeats".into());
        Timer::default()
            .interval(Duration::from_secs(1))
            .fold(0, move |i, _| {
                trace!("heartbeat");
                metrics.recorder().set(&heartbeats, i);
                Ok(i + 1)
            })
            .map(|_| {})
            .map_err(|_| {})
    };

    let mut core = Core::new().expect("Failed to create core");
    core.handle().spawn(heartbeat);
    core.handle().spawn(reporter(Duration::from_secs(2), report));
    core.run(aggregator).expect("heartbeat failed");

    work_thread.join().expect("work thread failed to join");
}


// Prints a report to stdout.
pub fn print_report(report: &tacho::Report) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(tacho::prometheus::format(report).as_bytes());
}

// Returns a Future that periodically prints a report to stdout.
pub fn reporter(interval: Duration,
                report: BiLock<tacho::Report>)
                -> Box<Future<Item = (), Error = ()>> {
    Timer::default()
        // TODO: make this configurable
        .interval(interval)
        .map_err(|_| ())
        .fold(report, move |report, _| {
            report.lock().map(move |mut report| {
                trace!("reporting");
                // TODO: this should write to an Arc<RwLock<String>> that's been
                // passed in or to a Sender<String> that's listening for new reports.
                print_report(&report);
                println!("");
                report.reset();
                report.unlock()
            })
        })
        .map(|_| {})
        .boxed()
}
