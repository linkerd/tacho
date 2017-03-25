//! A thread-safe, `Future`-aware metrics library.
//!
//! Many programs need to information about runtime performance: the number of requests
//! served, a distribution of request latency, the number of failures, the number of loop
//! iterations, etc. `tacho` allows application code to record runtime information to a central
//! `Aggregator` that merges data into a `Report`.
//!
//! ```
//! extern crate futures; extern crate tacho; extern crate tokio_core; extern crate
//! tokio_timer;
//!
//! use futures::{Future, future}; use std::time::Duration; use tokio_core::reactor::Core;
//! use tacho::{Scope, Tacho, Timing};
//! use tokio_timer::Timer;
//!
//! fn main() {
//!     let mut core = Core::new().expect("Failed to create core");
//!
//!     let Tacho { metrics, aggregator, report } = Tacho::default();
//!     core.handle().spawn(aggregator);
//!
//!     let metrics = metrics.labeled("labelkey".into(), "labelval".into());
//!     let iter_time_us = metrics.scope().stat("iter_time_us".into());
//!
//!     let timer = Timer::default();
//!     let working = future::loop_fn(99, move |n| {
//!         let metrics = metrics.clone();
//!         let iter_time_us = iter_time_us.clone();
//!         let start = Timing::start();
//!         timer.sleep(Duration::from_millis(n)).map_err(|_| {}).map(move |_| if n == 0 {
//!             future::Loop::Break(n)
//!         } else {
//!             metrics.recorder().add(&iter_time_us, start.elapsed_us());
//!             future::Loop::Continue(n - 1)
//!         })
//!     });
//!
//!     let reported = working.and_then(|_| {
//!         report.lock().map(|report| {
//!             // println!("{}", tacho::prometheus::format(&report));
//!         })
//!     });
//!     core.run(reported).expect("couldn't run tokio reactor");
//! }
//! ```
//!
//! ## Performance
//!
//! We found that the default (cryptographic) `HashMap` algorithm adds a significant
//! performance penalty. So the `RandomXxHashBuilder` algorithm is used to manage values
//! in
//!
//! Labels are stored in a `BTreeMap`, because they are used as keys in the `Report`'s
//! `HashMap` (and so we need to be able to derive `Hash` on the set of labels).

extern crate futures;
extern crate hdrsample;
extern crate tokio_timer;
#[macro_use]
extern crate log;
extern crate twox_hash;

use futures::sync::{BiLock, mpsc};

mod aggregator;
mod key;
pub mod prometheus;
mod recorder;
mod scope;
mod timing;

pub use aggregator::{Aggregator, Report};
pub use key::{CounterKey, GaugeKey, StatKey, MetricKey};
pub use recorder::{Recorder, RecorderFactory, Sample};
pub use scope::Scope;
pub use timing::Timing;

/// Limits the maximum number of `Samples` processed in a single invocation of `poll()`.
const AGGREGATOR_BATCH_SIZE: usize = 1_000;

/// A metrics pipeline.
///
/// Metrics are to be written into a `Receiver`, which sends raw data to a single
/// `Aggregator`, which publishes a `Report`.
pub struct Tacho {
    pub metrics: Metrics,
    pub aggregator: Aggregator,
    pub report: BiLock<Report>,
}
impl Default for Tacho {
    fn default() -> Tacho {
        Tacho::new(AGGREGATOR_BATCH_SIZE)
    }
}
impl Tacho {
    pub fn new(aggregator_batch_size: usize) -> Tacho {
        let (samples_tx, samples_rx) = mpsc::unbounded();
        let (report, agg_report) = BiLock::new(Report::default());
        Tacho {
            metrics: Metrics {
                recorder: recorder::factory(samples_tx),
                scope: Scope::default(),
            },
            aggregator: aggregator::new(samples_rx, agg_report, aggregator_batch_size),
            report: report,
        }
    }
}

#[derive(Clone)]
pub struct Metrics {
    recorder: RecorderFactory,
    scope: Scope,
}
impl Metrics {
    pub fn recorder(&self) -> Recorder {
        self.recorder.mk()
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn labeled(self, k: String, v: String) -> Metrics {
        let rec = self.recorder;
        let scope = self.scope;
        Metrics {
            recorder: rec,
            scope: scope.labeled(k, v),
        }
    }
}
