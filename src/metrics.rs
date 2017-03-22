extern crate futures;
extern crate hdrsample;
extern crate twox_hash;
extern crate tokio_core;

use tokio_timer::Timer as TokioTimer;

use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender, SendError};
use tokio_core::reactor::Handle;
use futures::{Future, Stream};
use futures::sync::BiLock;

use super::timer::Timer;
use super::counter::Counter;
use super::gauge::Gauge;
use super::reporter::print_report;

use hdrsample::Histogram;

use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::fmt;
use std::time::Duration;

use twox_hash::RandomXxHashBuilder;

pub struct Metrics {
    pub counter_store: HashMap<String, u64, RandomXxHashBuilder>,
    pub gauge_store: HashMap<String, u64, RandomXxHashBuilder>,
    pub timer_store: HashMap<String, Histogram<u64>, RandomXxHashBuilder>,
}

// A Synchronous Metrics implementation.
impl Metrics {
    pub fn new() -> Metrics {
        Metrics {
            counter_store: Default::default(),
            gauge_store: Default::default(),
            timer_store: Default::default(),
        }
    }

    /// Returns a Counter associated with this metrics object.
    pub fn make_counter(&mut self, name: String) -> Counter {
        let counter = Counter::new(name.clone());
        if !self.counter_store.contains_key(&name) {
            self.counter_store.insert(name, 0);
        }
        return counter;
    }

    pub fn make_timer(&mut self, name: String) -> Timer {
        let timer = Timer::new(name.clone());
        if !self.timer_store.contains_key(&name) {
            // TODO: this is one minute in microseconds.
            let histogram = Histogram::<u64>::new_with_bounds(1, 60 * 1000 * 1000, 5).unwrap();
            self.timer_store.insert(name, histogram);
        }
        return timer;
    }

    /// Adds the Counter value to the Metrics stored Counter value.
    /// Returns a fresh Counter.
    pub fn report_counter(&mut self, counter: Counter) -> Counter {
        if !self.counter_store.contains_key(&counter.name) {
            self.counter_store.insert(counter.name.clone(), 0);
        }
        if let Some(original_count) = self.counter_store.get_mut(&counter.name) {
            *original_count += counter.value;
        }
        return counter.fresh();
    }

    /// Adds the Counter value to the Metrics stored Counter value.
    /// Returns a fresh Counter.
    pub fn report_gauge(&mut self, gauge: Gauge) -> Gauge {
        self.counter_store.insert(gauge.name.clone(), gauge.value);
        return gauge.fresh();
    }

    /// Adds the Timer value to the Metrics stored Timer value.
    /// Returns a fresh Timer ready to be used.
    pub fn report_timer(&mut self, timer: Timer) -> Timer {
        if !self.timer_store.contains_key(&timer.name) {
            // TODO: this is one minute in microseconds.
            let histogram = Histogram::<u64>::new_with_bounds(1, 60 * 1000 * 1000, 5).unwrap();
            self.timer_store.insert(timer.name.clone(), histogram);
        }
        if let Some(histogram) = self.timer_store.get_mut(&timer.name) {
            if let Some(elapsed) = timer.elapsed {
                let _ = histogram.record(elapsed);
            }
        }
        return timer.fresh();
    }

    pub fn clear(&mut self) {
        self.counter_store.clear();
        self.gauge_store.clear();
        self.timer_store.clear();
    }
}

#[derive(Clone, Debug)]
pub struct MetricsBundle {
    pub counters: Vec<Counter>,
    pub gauges: Vec<Gauge>,
    pub timers: Vec<Timer>,
}

/// A MetricsBundle is a simplifying abstraction for writing a collection
/// of Metrics into an AsyncMetrics tx.
impl MetricsBundle {
    pub fn new() -> MetricsBundle {
        MetricsBundle {
            counters: vec![],
            gauges: vec![],
            timers: vec![],
        }
    }

    pub fn with_metrics(counters: Vec<Counter>,
                        gauges: Vec<Gauge>,
                        timers: Vec<Timer>)
                        -> MetricsBundle {
        MetricsBundle {
            counters: counters,
            gauges: gauges,
            timers: timers,
        }
    }

    /// Places the current MetricsBundle into the tx receiver for later processing.
    pub fn report(self,
                  mut tx: UnboundedSender<MetricsBundle>)
                  -> Result<(), SendError<MetricsBundle>> {
        UnboundedSender::send(&mut tx, self)
    }
}

impl fmt::Display for MetricsBundle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({:?}, {:?})", self.counters, self.timers)
    }
}

/// For interfacing with tokio services, we provide a Futures-aware mpsc Channel
/// for writing metrics into.
///
pub struct AsyncMetrics {
    pub metrics: Metrics,
    pub tx: UnboundedSender<MetricsBundle>,
}

impl AsyncMetrics {
    /// Returns an AsyncMetrics object for sending metrics and a separate UnboundedReceiver<MetricsBundle>
    /// for processing new Metrics for storing in the AsyncMetrics store.
    pub fn new() -> (AsyncMetrics, UnboundedReceiver<MetricsBundle>) {
        let (tx, rx) = unbounded();

        (AsyncMetrics {
            metrics: Metrics::new(),
            tx: tx, // rx: Arc::new(RwLock::new(rx)),
        },
         rx)
    }
}

// Important to note: If you put enough items into this Receiver,
// the event loop will spend all of it's time processing those items
// and never give other Futures time to work.
pub fn aggregator(rx: UnboundedReceiver<MetricsBundle>,
                  lock: BiLock<AsyncMetrics>,
                  handle: &Handle) {
    let aggregator = rx.fold(lock, |aggregator_lock, bundle| {
        aggregator_lock.lock().map(move |mut async_metrics| {
            for timer in bundle.timers.iter() {
                async_metrics.metrics.report_timer((*timer).clone());
            }

            for counter in bundle.counters.iter() {
                async_metrics.metrics.report_counter((*counter).clone());
            }

            for gauge in bundle.gauges.iter() {
                async_metrics.metrics.report_gauge((*gauge).clone());
            }
            async_metrics.unlock()
        })
    });

    handle.spawn_fn(move || {
        aggregator.then(move |_| {
            debug!("aggregator has stopped");
            Ok(()) as Result<(), ()>
        })
    });
}

pub fn report_generator(reporter_lock: BiLock<AsyncMetrics>, handle: &Handle) {
    let report_generator = TokioTimer::default()
        .interval(Duration::from_millis(1000 * 2))
        .map_err(|_| ())
        .fold(reporter_lock, move |reporter_lock, _| {
            trace!("making report");
            reporter_lock.lock().map(move |mut reporter_lock| {
                print_report(&reporter_lock.metrics);
                println!("");
                reporter_lock.metrics.clear();
                reporter_lock.unlock()
            })
        })
        .map_err(|_| Error::new(ErrorKind::Other, "unable to run report generator"));

    handle.spawn_fn(move || {
        report_generator.then(move |_| {
            debug!("report generator has stopped.");
            Ok(()) as Result<(), ()>
        })
    });
}

#[cfg(test)]
mod tests {
    use super::Metrics;
    #[test]
    fn test_basic_metrics_1() {
        let metrics = Metrics::new();
    }
}
