extern crate log;
extern crate env_logger;
extern crate futures;
extern crate tachograph;
extern crate tokio_core;
extern crate tokio_timer;

use futures::Stream;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use std::thread;
use tokio_core::reactor::Core;
use tokio_timer::Timer as TokioTimer;

use tachograph::{Counter, Gauge};
use tachograph::timer::Timer;
use tachograph::metrics;

// A performance test for an asynchronous Metrics reporter with timers and counts.
fn main() {
    drop(env_logger::init());

    let (recorder, aggregator) = metrics::new();

    let work_thread = {
        let mut tx = recorder.clone();
        let mut total_timer = Timer::new("total_time_us".to_owned());
        thread::spawn(move || {
            for i in 0..100_000_000 {
                if i % 100 == 0 {
                    thread::sleep(Duration::from_millis(1));
                }
                let mut loop_timer = Timer::new("loop_timer_us".to_owned());
                let mut loop_counter = Counter::new("total_loops".to_owned(), 0);
                let loop_gauge = Gauge::new("loop_iter".to_owned(), i);
                loop_timer.start();
                loop_counter.incr(1);
                // Do your work here
                loop_timer.stop();
                tx.record(vec![loop_counter], vec![loop_gauge], vec![loop_timer]);
            }
            println!("WOW THAT WAS FAST!");
            total_timer.stop();
            tx.record(vec![], vec![], vec![total_timer])
        })
    };

    let mut core = Core::new().expect("Failed to create core");
    let handle = core.handle();
    let (aggregated, aggregating) = aggregator.aggregate();
    handle.spawn(aggregating);
    handle.spawn(metrics::report_generator(aggregated));

    let mut tx = recorder.clone();
    let mut heartbeats = 0;
    let heartbeater = TokioTimer::default()
        .interval(Duration::from_millis(1000))
        .map_err(|_| Error::new(ErrorKind::Other, "unable to run report generator"))
        .for_each(|_| {
            heartbeats += 1;
            let heartbeats_gauge = Gauge::new("heartbeats".to_owned(), heartbeats);
            tx.record(vec![], vec![heartbeats_gauge], vec![]);
            Ok(()) as Result<(), std::io::Error>
        });
    core.run(heartbeater).expect("heartbeat failed");
    work_thread.join().expect("work thread failed to join");
}
