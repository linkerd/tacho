//! `Future`-powered Metrics collection and reporting
//!
//!
//! Example use within a mixed multi-threaded app with tokio
//!
//! ```no_run
//! extern crate log;
//! extern crate env_logger;
//! extern crate futures;
//! extern crate tacho;
//! extern crate tokio_core;
//! extern crate tokio_timer;
//!
//! use futures::Stream;
//! use std::io::{Error, ErrorKind};
//! use std::time::Duration;
//! use std::thread;
//! use tokio_core::reactor::Core;
//! use tokio_timer::Timer as TokioTimer;
//!
//! use tacho::{Counter, Gauge};
//! use tacho::timer::Timer;
//! use tacho::metrics;
//!
//! // A performance test for an asynchronous Metrics reporter with timers, counters, and gauges.
//! fn main() {
//!     drop(env_logger::init());
//!
//!     let (recorder, aggregator) = metrics::new();
//!
//!     let work_thread = {
//!         let mut tx = recorder.clone();
//!         let mut total_timer = Timer::new("total_time_us".to_owned());
//!         thread::spawn(move || {
//!             for i in 0..100_000_000 {
//!                 if i % 100 == 0 {
//!                     thread::sleep(Duration::from_millis(1));
//!                 }
//!                 let mut loop_timer = Timer::new("loop_timer_us".to_owned());
//!                 let mut loop_counter = Counter::new("total_loops".to_owned(), 0);
//!                 let loop_gauge = Gauge::new("loop_iter".to_owned(), i);
//!                 loop_timer.start();
//!                 loop_counter.incr(1);
//!                 // Do your work here
//!                 loop_timer.stop();
//!                 tx.record(vec![loop_counter], vec![loop_gauge], vec![loop_timer]);
//!             }
//!             total_timer.stop();
//!             tx.record(vec![], vec![], vec![total_timer])
//!         })
//!     };
//!
//!     let mut core = Core::new().expect("Failed to create core");
//!     let handle = core.handle();
//!     let (aggregated, aggregating) = aggregator.aggregate();
//!     handle.spawn(aggregating);
//!     handle.spawn(metrics::report_generator(aggregated));
//!
//!     let mut tx = recorder.clone();
//!     let mut heartbeats = 0;
//!     let heartbeater = TokioTimer::default()
//!         .interval(Duration::from_millis(1000))
//!         .map_err(|_| Error::new(ErrorKind::Other, "unable to run heartbeat"))
//!         .for_each(|_| {
//!             heartbeats += 1;
//!             let heartbeats_gauge = Gauge::new("heartbeats".to_owned(), heartbeats);
//!             tx.record(vec![], vec![heartbeats_gauge], vec![]);
//!             Ok(()) as Result<(), std::io::Error>
//!         });
//!     core.run(heartbeater).expect("heartbeat failed");
//!     work_thread.join().expect("work thread failed to join");
//! }
//! ```

extern crate chrono;
extern crate futures;
extern crate hdrsample;
extern crate tokio_core;
extern crate tokio_timer;
#[macro_use]
extern crate log;
extern crate twox_hash;

use std::collections::VecDeque;

pub mod metrics;
pub mod reporter;
pub mod timer;

pub use metrics::{Metrics, Recorder};
pub use timer::Timer as Timed;

// Counters are monotonically increasing values.
#[derive(Clone, Debug)]
pub struct Counter {
    pub name: String,
    pub value: u64,
}

impl Counter {
    // Creates a Counter with a given name and initial value.
    pub fn new(name: String, init: u64) -> Counter {
        Counter {
            name: name,
            value: init,
        }
    }

    pub fn incr(&mut self, n: u64) {
        self.value += n;
    }

    pub fn fresh(&self) -> Counter {
        Counter {
            name: self.name.clone(),
            value: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Gauge {
    pub name: String,
    pub value: u64,
}

impl Gauge {
    pub fn new(name: String, n: u64) -> Gauge {
        Gauge {
            name: name,
            value: n,
        }
    }

    pub fn set(&mut self, n: u64) {
        self.value = n;
    }
}

pub struct Stat {
    pub name: String,
    pub values: VecDeque<u64>,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_incr() {
        let mut counter = Counter::new("foo".to_owned(), 3);
        counter.incr(1);
        counter.incr(1);
        counter.incr(1);
        assert!(counter.value == 6);
    }

    #[test]
    fn test_basic_gauges() {
        let mut v = Gauge::new("foo".into(), 123);
        v.set(432);
        assert_eq!(v.value, 432);
    }
}
