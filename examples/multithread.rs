//! A performance test for stats being accessed across threads.

#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate futures;
extern crate tacho;
extern crate tokio_core;
extern crate tokio_timer;

use futures::{BoxFuture, Future, Stream};
use futures::sync::oneshot;
use std::thread;
use std::time::Duration;
use tacho::Timing;
use tokio_core::reactor::Core;
use tokio_timer::Timer;

fn main() {
    drop(pretty_env_logger::init());

    let (metrics, report) = tacho::new();

    let (work_done_tx0, work_done_rx0) = oneshot::channel();
    let (work_done_tx1, work_done_rx1) = oneshot::channel();
    let reporter = {
        let work_done_rx = work_done_rx0
            .join(work_done_rx1)
            .map(|_| {})
            .map_err(|_| ());
        let interval = Duration::from_secs(2);
        reporter(interval, work_done_rx, report)
    };

    let metrics = metrics.clone().labeled("test", "multithread_stat".into());
    let loop_iter_us = metrics.stat("loop_iter_us");
    for (i, work_done_tx) in vec![(0, work_done_tx0), (1, work_done_tx1)] {
        let metrics = metrics.clone().labeled("thread", format!("{}", i));
        let loop_counter = metrics.counter("loop_counter");
        let current_iter = metrics.gauge("current_iter");
        let loop_iter_us = loop_iter_us.clone();
        thread::spawn(move || {
            let mut prior = None;
            for i in 0..10_000_000 {
                let t0 = Timing::start();
                current_iter.set(i);
                loop_counter.incr(1);
                if let Some(p) = prior {
                    loop_iter_us.add(p);
                }
                prior = Some(t0.elapsed_us());
            }
            if let Some(p) = prior {
                loop_iter_us.add(p);
            }

            work_done_tx.send(()).expect("could not send");
        });
    }

    let mut core = Core::new().expect("Failed to create core");
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
    let done = done.map(move |_| { print_report(&reporter.peek()); });
    periodic.select(done).map(|_| {}).map_err(|_| {}).boxed()
}

fn print_report(report: &tacho::Report) {
    let out = tacho::prometheus::string(report).unwrap();
    info!("\n{}", out);
}
