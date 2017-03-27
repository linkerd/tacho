extern crate pretty_env_logger;
extern crate futures;
extern crate tacho;
extern crate tokio_core;
extern crate tokio_timer;

use futures::{Future, future};
use std::time::Duration;
use tacho::{Tacho, Timing};
use tokio_core::reactor::Core;
use tokio_timer::Timer;

fn main() {
    drop(pretty_env_logger::init());

    // Create a new tacho pipeline.
    let Tacho { metrics, aggregator, report } = Tacho::default();

    let reported = do_work(metrics).and_then(move |_| {
        Timer::default()
            .sleep(Duration::from_millis(1000))
            .map_err(|_| {})
            .and_then(|_| {
                report.lock().map(|report| {
                    println!("# metrics:");
                    println!("");
                    println!("{}", tacho::prometheus::format(&report));
                })
            })
    });

    let mut core = Core::new().expect("could not create core");
    core.handle().spawn(aggregator);
    core.run(reported).expect("reactor failed");
}

fn do_work(metrics: tacho::Metrics) -> future::BoxFuture<(), ()> {
    let metrics = metrics.labeled("labelkey".into(), "labelval".into());
    let iter_time_us = metrics.scope().stat("iter_time_us".into());
    let timer = Timer::default();
    future::loop_fn(100, move |n| {
            // Clones are shallow, minimizing allocation.
            let iter_time_us = iter_time_us.clone();

            // An RAII-gaurded recorder flushes data
            let mut recorder = metrics.recorder();

            let start = Timing::start();
            timer.sleep(Duration::from_millis(20 * (n % 5)))
                .map_err(|_| {})
                .map(move |_| if n == 0 {
                    future::Loop::Break(n)
                } else {
                    recorder.add(&iter_time_us, start.elapsed_us());
                    future::Loop::Continue(n - 1)
                })
        })
        .map(|_| {})
        .boxed()
}
