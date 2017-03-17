extern crate futures;
extern crate hdrsample;
extern crate twox_hash;
extern crate tokio_core;

use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender, SendError};

use timer::Timer;
use counter::Counter;
use hdrsample::Histogram;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::fmt;
use twox_hash::RandomXxHashBuilder;

pub struct Metrics {
    pub counter_store: HashMap<String, u64, RandomXxHashBuilder>,
    pub timer_store: HashMap<String, Histogram<u64>, RandomXxHashBuilder>,
}

// A Synchronous Metrics implementation.
impl Metrics {
    pub fn new() -> Metrics {
        Metrics {
            counter_store: Default::default(),
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
}

#[derive(Clone, Debug)]
pub struct MetricsBundle {
    pub counters: Vec<Counter>,
    pub timers: Vec<Timer>,
}

/// A MetricsBundle is a simplifying abstraction for writing a collection
/// of Metrics into an AsyncMetrics tx.
impl MetricsBundle {
    pub fn new() -> MetricsBundle {
        MetricsBundle {
            counters: vec![],
            timers: vec![],
        }
    }

    pub fn with_metrics(counters: Vec<Counter>, timers: Vec<Timer>) -> MetricsBundle {
        MetricsBundle {
            counters: counters,
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
    pub metrics: Arc<RwLock<Metrics>>,
    pub tx: UnboundedSender<MetricsBundle>,
}

impl AsyncMetrics {
    /// Returns an AsyncMetrics object for sending metrics and a separate UnboundedReceiver<MetricsBundle>
    /// for processing new Metrics for storing in the AsyncMetrics store.
    pub fn new() -> (AsyncMetrics, UnboundedReceiver<MetricsBundle>) {
        let (tx, rx) = unbounded();

        (AsyncMetrics {
            metrics: Arc::new(RwLock::new(Metrics::new())),
            tx: tx, // rx: Arc::new(RwLock::new(rx)),
        },
         rx)
    }
}

#[cfg(test)]
mod tests {
    use super::Metrics;
    #[test]
    fn test_basic_metrics_1() {
        let metrics = Metrics::new();
    }
}
