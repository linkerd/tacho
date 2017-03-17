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
use tachograph::timer::Timer as TTimer;
use tachograph::metrics::{AsyncMetrics, MetricsBundle};
use tachograph::reporter;

use futures::stream::Stream;
use futures::future::{ok, lazy, Future};
use futures::IntoFuture;
use std::time::Duration;
use tokio_core::reactor::Core;
use tokio_timer::*;

use std::sync::{Arc, RwLock};
use std::thread;

// A performance test for an asynchronous Metrics reporter with timers and counts.
fn main() {
    let (async_metrics, rx) = AsyncMetrics::new();
    let async_metrics = Arc::new(RwLock::new(async_metrics));
    drop(env_logger::init());
    let mut core = Core::new().expect("Failed to create core");

    let async_metrics_lock1 = async_metrics.clone();
    let tx = async_metrics_lock1.read().unwrap().tx.clone();

    let mut total_timer = TTimer::new("total_time_us".to_owned());
    total_timer.start();

    let remote = core.remote().clone();
    let tx = tx.clone();
    let tx_for_worker = tx.clone();
    thread::spawn(move || {
        let worker = futures::stream::iter((0..1000).map(|_| Ok(()) as Result<(), ()>))
            .for_each(move |_| {
                let tx = tx_for_worker.clone();
                let mut loop_timer = TTimer::new("loop_timer_us".to_owned());
                let mut loop_counter = Counter::new("total_loops".to_owned());
                loop_timer.start();
                loop_counter.incr();
                // Do your work here
                loop_timer.stop();
                let _ = MetricsBundle::with_metrics(vec![loop_counter], vec![loop_timer])
                    .report(tx);
                Ok(()) as Result<(), ()>
            })
            .into_future();
        remote.spawn(move |handle| {
            ok(handle.spawn_fn(move || {
                worker.then(move |_| {
                    info!("worker finished its run");
                    Ok(()) as Result<(), ()>
                })
            }))
        })
    });

    total_timer.stop();
    assert!(MetricsBundle::with_metrics(vec![], vec![total_timer]).report(tx.clone()).is_ok());

    info!("going to run the aggregator");
    let timer = Timer::default();

    let remote = core.remote().clone();
    let async_metrics_lock = async_metrics.clone();
    let t = thread::spawn(move || {
        let metrics = async_metrics_lock.read().unwrap().metrics.clone();
        let aggregator = rx.for_each(move |bundle| {
                let metrics_lock = metrics.clone();
                let mut metrics = metrics_lock.write().unwrap();
                for timer in bundle.timers.iter() {
                    metrics.report_timer((*timer).clone());
                }
                for counter in bundle.counters.iter() {
                    metrics.report_counter((*counter).clone());
                }
                Ok(()) as Result<(), ()>
            })
            .into_future();

        remote.spawn(move |handle| {
            ok(handle.spawn_fn(move || {
                aggregator.then(move |_| {
                    debug!("aggregator finished its run");
                    Ok(()) as Result<(), ()>
                })
            }))
        })
    });

    let one_minute_sleep = timer.sleep(Duration::from_millis(1000 * 60));
    let foo = one_minute_sleep.then(|_| {
        lazy(|| {
            let async_metrics = (*async_metrics).read().unwrap();
            let metrics_lock_guard = async_metrics.metrics.read();
            match metrics_lock_guard {
                Ok(metrics) => reporter::print_report(&metrics),
                Err(e) => error!("failed to get async_metrics.metrics for reporting {}", e),
            }
            Ok(()) as Result<(), ()>
        })
    });

    assert!(core.run(foo).is_ok());
    t.join().unwrap();
}
