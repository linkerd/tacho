#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate tokio_timer;

extern crate tachograph;
use tachograph::counter::Counter;
use tachograph::gauge::Gauge;
use tachograph::timer::Timer;
use tachograph::metrics::{AsyncMetrics, MetricsBundle, aggregator, report_generator};

use futures::sync::BiLock;

use futures::Stream;
use futures::future::ok;
use std::time::Duration;
use tokio_core::reactor::Core;
use tokio_timer::Timer as TokioTimer;
use std::thread;
use std::io::{Error, ErrorKind};

// A performance test for an asynchronous Metrics reporter with timers and counts.
fn main() {
    drop(env_logger::init());

    let (async_metrics, rx) = AsyncMetrics::new();
    let tx = async_metrics.tx.clone();
    let (aggregator_lock, reporter_lock) = BiLock::new(async_metrics);
    let mut core = Core::new().expect("Failed to create core");

    let remote = core.remote().clone();
    remote.spawn(move |handle| ok(aggregator(rx, aggregator_lock, handle)));

    let tx = tx.clone();
    let tx_for_worker = tx.clone();
    let mut total_timer = Timer::new("total_time_us".to_owned());

    let _ = thread::spawn(move || {
        for i in 0..100_000_000 {
            if i % 100 == 0 {
                thread::sleep(Duration::from_millis(1));
            }
            let mut loop_timer = Timer::new("loop_timer_us".to_owned());
            let mut loop_counter = Counter::new("total_loops".to_owned());
            let loop_gauge = Gauge::new("loop_iter".to_owned(), i);
            loop_timer.start();
            loop_counter.incr();
            // Do your work here
            loop_timer.stop();
            let _ =
                MetricsBundle::with_metrics(vec![loop_counter], vec![loop_gauge], vec![loop_timer])
                    .report(tx_for_worker.clone());
        }
        println!("WOW THAT WAS FAST!");
        total_timer.stop();
        MetricsBundle::with_metrics(vec![], vec![], vec![total_timer])
            .report(tx_for_worker)
            .expect("jj");
    });

    let handle = core.handle();
    report_generator(reporter_lock, &handle);

    let tx_for_heartbeat = tx.clone();
    let mut heartbeats = 0;
    let heartbeater = TokioTimer::default()
        .interval(Duration::from_millis(1000))
        .map_err(|_| Error::new(ErrorKind::Other, "unable to run report generator"))
        .for_each(|_| {
            heartbeats += 1;
            let heartbeats_gauge = Gauge::new("heartbeats".to_owned(), heartbeats.clone());
            let _ = MetricsBundle::with_metrics(vec![], vec![heartbeats_gauge], vec![])
                .report(tx_for_heartbeat.clone());
            Ok(()) as Result<(), std::io::Error>
        });
    core.run(heartbeater).expect("heartbeat failed");
}
