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
use futures::future::Future;
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

    let loop_counter = Counter::new("total_loops".to_owned());
    let loop_timer = TTimer::new("loop_timer_us".to_owned());
    let mut total_timer = TTimer::new("total_time_us".to_owned());
    total_timer.start();

    let tx2 = tx.clone();
    let worker = futures::stream::iter((0..1_000).map(Ok))
        .then(|x| futures::done::<u32, ()>(x))
        .map(|_| {
            let tx = tx2.clone();
            let mut loop_timer = loop_timer.clone();
            let mut loop_counter = loop_counter.clone();
            loop_timer.start();
            loop_counter.incr();
            // Do your work here
            loop_timer.stop();
            info!("{:?}", loop_counter);
            MetricsBundle::with_metrics(vec![loop_counter], vec![loop_timer]).report(tx)
        })
        .collect()
        .into_future();

    assert!(core.run(worker).is_ok());
    total_timer.stop();
    MetricsBundle::with_metrics(vec![], vec![total_timer]).report(tx.clone());

    info!("going to run the aggregator");
    let timer = Timer::default();

    let remote = core.remote().clone();
    let async_metrics_lock = async_metrics.clone();
    thread::spawn(move || {
        // NOTE: This aggregator won't run.
        info!("Spawned Thread going to spawn a aggregator");
        let metrics = async_metrics_lock.read().unwrap().metrics.clone();
        let aggregator = rx.for_each(move |bundle| {
                info!("can we get a lock?");
                let metrics_lock = metrics.clone();
                let mut metrics = metrics_lock.write().unwrap();
                for timer in bundle.timers.iter() {
                    metrics.report_timer((*timer).clone());
                }
                for counter in bundle.counters.iter() {
                    metrics.report_counter((*counter).clone());
                }
                info!("aggregator sees: {:?}", bundle);
                let result: Result<(), ()> = Ok(());
                result
            })
            .into_future();
        remote.spawn(|handle| {
            handle.spawn(aggregator);
            futures::future::ok(())
        });
    });

    let one_minute_sleep = timer.sleep(Duration::from_millis(1000 * 3));
    info!("Going to run our one_minute_sleep and then aggregate");
    let _ = one_minute_sleep.wait();
    info!("Woke up from our sleep");
    info!("YOYO going to print some stuff. hold onto your butts");
    let async_metrics = (*async_metrics).read().unwrap();
    let metrics_lock_guard = async_metrics.metrics.read();
    match metrics_lock_guard {
        Ok(metrics) => reporter::print_report(&metrics),
        Err(e) => error!("failed to get async_metrics.metrics for reporting {}", e),
    }

}
