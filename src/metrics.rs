extern crate futures;
extern crate hdrsample;
extern crate twox_hash;
extern crate tokio_core;

use futures::{Future, Stream};
use futures::sync::{BiLock, mpsc};
use hdrsample::Histogram;
use std::collections::HashMap;
use std::time::Duration;

use super::{Counter, Gauge};
use super::reporter::print_report;
use super::timer::Timer;
use tokio_timer::Timer as TokioTimer;
use twox_hash::RandomXxHashBuilder;

pub fn new() -> (Recorder, Aggregator) {
    let (tx, rx) = mpsc::unbounded();
    (Recorder(tx), Aggregator(rx))
}

pub struct Metrics {
    pub counter_store: HashMap<String, u64, RandomXxHashBuilder>,
    pub gauge_store: HashMap<String, u64, RandomXxHashBuilder>,
    pub timer_store: HashMap<String, Histogram<u64>, RandomXxHashBuilder>,
}

impl Default for Metrics {
    fn default() -> Metrics {
        Metrics {
            counter_store: Default::default(),
            gauge_store: Default::default(),
            timer_store: Default::default(),
        }
    }
}

// A Synchronous Metrics implementation.
impl Metrics {
    pub fn new() -> Metrics {
        Default::default()
    }

    /// Returns a Counter associated with this metrics object.
    pub fn make_counter(&mut self, name: String) -> Counter {
        let counter = Counter::new(name.clone(), 0);
        self.counter_store.entry(name).or_insert(0);
        counter
    }

    pub fn make_timer(&mut self, name: String) -> Timer {
        let timer = Timer::new(name.clone());
        // TODO: this is one minute in microseconds.
        self.timer_store
            .entry(name)
            .or_insert_with(|| Histogram::<u64>::new_with_bounds(1, 60 * 1000 * 1000, 5).unwrap());
        timer
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
        counter.fresh()
    }

    /// Adds the Counter value to the Metrics stored Counter value.
    /// Returns a fresh Counter.
    pub fn report_gauge(&mut self, gauge: Gauge) {
        self.gauge_store.insert(gauge.name.clone(), gauge.value);
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
        timer.fresh()
    }

    pub fn clear(&mut self) {
        self.gauge_store.clear();
        self.timer_store.clear();
    }
}

#[derive(Clone, Debug)]
struct MetricsBundle {
    counters: Vec<Counter>,
    gauges: Vec<Gauge>,
    timers: Vec<Timer>,
}

/// A `MetricsBundle` is used to pass a group of metrics from the `Recorder` to the `Aggregator`.
impl MetricsBundle {
    pub fn new(counters: Vec<Counter>, gauges: Vec<Gauge>, timers: Vec<Timer>) -> MetricsBundle {
        MetricsBundle {
            counters: counters,
            gauges: gauges,
            timers: timers,
        }
    }
}

#[derive(Clone)]
pub struct Recorder(mpsc::UnboundedSender<MetricsBundle>);
impl Recorder {
    /// Places the current MetricsBundle into the tx receiver for later processing.
    pub fn record(&mut self, counters: Vec<Counter>, gauges: Vec<Gauge>, timers: Vec<Timer>) {
        let mb = MetricsBundle::new(counters, gauges, timers);
        if mpsc::UnboundedSender::send(&self.0, mb).is_err() {
            info!("dropping metrics");
        }
    }
}

pub struct Aggregator(mpsc::UnboundedReceiver<MetricsBundle>);
impl Aggregator {
    // Important to note: If you put enough items into this Receiver,
    // the event loop will spend all of its time processing those items
    // and never give other Futures time to work.
    pub fn aggregate(self) -> (BiLock<Metrics>, Box<Future<Item = (), Error = ()>>) {
        let (aggregated, reporter) = BiLock::new(Metrics::new());
        let done = self.0
            .fold(aggregated, |aggregated, bundle| {
                aggregated.lock().map(move |mut aggregated| {
                    for timer in &bundle.timers {
                        aggregated.report_timer((*timer).clone());
                    }

                    for counter in &bundle.counters {
                        aggregated.report_counter((*counter).clone());
                    }

                    for gauge in &bundle.gauges {
                        aggregated.report_gauge((*gauge).clone());
                    }
                    aggregated.unlock()
                })
            })
            .map(|_| {})
            .boxed();
        (reporter, done)
    }
}

// Returns a Future that periodically prints a report to stdout.
pub fn report_generator(metrics: BiLock<Metrics>) -> Box<Future<Item = (), Error = ()>> {
    TokioTimer::default()
        // TODO: make this configurable
        .interval(Duration::from_millis(1000 * 2))
        .map_err(|_| ())
        .fold(metrics, move |metrics, _| {
            trace!("making report");
            metrics.lock().map(move |mut metrics| {
                // TODO: this should write to an Arc<RwLock<String>> that's been
                // passed in or to a Sender<String> that's listening for new reports.
                print_report(&metrics);
                println!("");
                metrics.clear();
                metrics.unlock()
            })
        })
        .map(|_| {})
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::Metrics;
    #[test]
    fn test_basic_metrics_1() {
        let _ = Metrics::new();
    }
}
