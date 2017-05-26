//! A thread-safe, `Future`-aware metrics library.
//!
//! Many programs need to information about runtime performance: the number of requests
//! served, a distribution of request latency, the number of failures, the number of loop
//! iterations, etc. `tacho` allows application code to record runtime information to a
//! central `Aggregator` that merges data into a `Report`.
//!
//! ## Performance
//!
//! We found that the default (cryptographic) `Hash` algorithm adds a significant
//! performance penalty, so the (non-cryptographic) `RandomXxHashBuilder` algorithm is
//! used..
//!
//! Labels are stored in a `BTreeMap` because they are used as hash keys and, therefore,
//! need to implement `Hash`.

// TODO use atomics when we have them.

#![feature(integer_atomics)]

extern crate hdrsample;
#[macro_use]
extern crate log;
extern crate twox_hash;
extern crate ordermap;

use hdrsample::Histogram;
use ordermap::OrderMap;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use twox_hash::RandomXxHashBuilder;

pub mod prometheus;
mod report;
mod timing;

pub use report::{Reporter, Report, ReportTake, ReportPeek};
pub use timing::Timing;

type Labels = BTreeMap<String, String>;
type CounterMap = OrderMap<Key, Arc<AtomicU64>, RandomXxHashBuilder>;
type GaugeMap = OrderMap<Key, Arc<AtomicU64>, RandomXxHashBuilder>;
type StatMap = OrderMap<Key, Histogram<u64>, RandomXxHashBuilder>;

/// Creates a metrics registry.
///
/// The returned `Scope` may be you used to instantiate metrics. Labels may be attached to
/// the scope so that all metrics created by this `Scope` are annotated.
///
/// The returned `Reporter` supports consumption of metrics values.
pub fn new() -> (Scope, Reporter) {
    let counters = Arc::new(Mutex::new(CounterMap::default()));
    let gauges = Arc::new(Mutex::new(GaugeMap::default()));
    let stats = Arc::new(Mutex::new(StatMap::default()));

    let scope = Scope {
        labels: Labels::default(),
        counters: counters.clone(),
        gauges: gauges.clone(),
        stats: stats.clone(),
    };

    let reporter = report::new(counters, gauges, stats);

    (scope, reporter)
}

/// Describes a metric.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    name: String,
    labels: Labels,
}
impl Key {
    fn new(name: String, labels: Labels) -> Key {
        Key {
            name: name,
            labels: labels,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}


/// Supports creation of scoped metrics.
///
/// `Scope`s may be cloned without copying the underlying metrics registry.
///
/// Labels may be attached to the scope so that all metrics created by the `Scope` are
/// labeled.
#[derive(Clone)]
pub struct Scope {
    labels: Labels,
    counters: Arc<Mutex<CounterMap>>,
    gauges: Arc<Mutex<GaugeMap>>,
    stats: Arc<Mutex<StatMap>>,
}

impl Scope {
    /// Accesses scoping labels.
    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    /// Adds a label into scope (potentially overwriting).
    pub fn labeled(self, k: String, v: String) -> Scope {
        Scope {
            counters: self.counters,
            gauges: self.gauges,
            stats: self.stats,
            labels: {
                let mut labels = self.labels;
                labels.insert(k, v);
                labels
            },
        }
    }

    /// Creates a Counter with the given name.
    pub fn counter(&self, name: String) -> Counter {
        let key = Key::new(name, self.labels.clone());
        let mut counters = self.counters
            .lock()
            .expect("failed to obtain lock on counters");
        let counter = counters
            .entry(key.clone())
            .or_insert_with(Default::default)
            .clone();
        Counter { key, counter }
    }

    /// Creates a Gauge with the given name.
    pub fn gauge(&self, name: String) -> Gauge {
        let key = Key::new(name, self.labels.clone());
        let mut gauges = self.gauges.lock().expect("failed to obtain lock on gauges");
        let gauge = gauges
            .entry(key.clone())
            .or_insert_with(Default::default)
            .clone();
        Gauge { key, gauge }
    }

    /// Creates a Stat with the given name.
    ///
    /// The underlying histogram is automatically resized as values are added.
    pub fn stat(&self, name: String) -> Stat {
        Stat {
            key: Key::new(name, self.labels.clone()),
            stats: self.stats.clone(),
            bounds: None,
        }
    }

    /// Creates a Stat with the given name and histogram paramters.
    pub fn stat_with_bounds(&self, name: String, low: u64, high: u64) -> Stat {
        Stat {
            key: Key::new(name, self.labels.clone()),
            stats: self.stats.clone(),
            bounds: Some((low, high)),
        }
    }
}

/// Counts monotically.
#[derive(Clone)]
pub struct Counter {
    key: Key,
    counter: Arc<AtomicU64>,
}
impl Counter {
    pub fn name(&self) -> &str {
        &self.key.name
    }
    pub fn labels(&self) -> &Labels {
        &self.key.labels
    }

    pub fn incr(&self, v: u64) {
        self.counter.fetch_add(v, Ordering::Relaxed);
    }
}

/// Captures an instantaneous value.
#[derive(Clone)]
pub struct Gauge {
    key: Key,
    gauge: Arc<AtomicU64>,
}
impl Gauge {
    pub fn name(&self) -> &str {
        &self.key.name
    }
    pub fn labels(&self) -> &Labels {
        &self.key.labels
    }

    pub fn set(&self, v: u64) {
        self.gauge.store(v, Ordering::Relaxed);
    }
}

/// Caputres a distribution of values.
#[derive(Clone)]
pub struct Stat {
    key: Key,
    stats: Arc<Mutex<StatMap>>,
    bounds: Option<(u64, u64)>,
}

const HISTOGRAM_PRECISION: u32 = 4;

impl Stat {
    pub fn name(&self) -> &str {
        &self.key.name
    }
    pub fn labels(&self) -> &Labels {
        &self.key.labels
    }

    pub fn add(&mut self, v: u64) {
        self.add_values(&[v]);
    }

    pub fn add_values(&mut self, vs: &[u64]) {
        trace!("histo record {:?} {:?}", self.key, vs);
        let mut stats = self.stats
            .lock()
            .expect("failed to obtain write lock for stat");
        if let Some(mut histo) = stats.get_mut(&self.key) {
            for v in vs {
                if let Err(e) = histo.record(*v) {
                    error!("failed to add value to histogram: {:?}", e);
                }
            }
            return;
        }

        trace!("creating histogram {:?} bounds={:?}", self.key, self.bounds);
        let mut histo = match self.bounds {
            None => Histogram::<u64>::new(HISTOGRAM_PRECISION).expect("failed to build Histogram"),
            Some((low, high)) => {
                Histogram::<u64>::new_with_bounds(low, high, HISTOGRAM_PRECISION)
                    .expect("failed to build Histogram")
            }
        };
        for v in vs {
            if let Err(e) = histo.record(*v) {
                error!("failed to add value to histogram: {:?}", e);
            }
        }
        stats.insert(self.key.clone(), histo);
    }
}

#[cfg(test)]
mod tests {
    use super::Report;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_report_peek() {
        let (metrics, reporter) = super::new();
        let metrics = metrics.labeled("joy".into(), "painting".into());

        metrics.counter("happy_accidents".into()).incr(1);
        metrics.gauge("paint_level".into()).set(2);
        metrics.stat("stroke_len".into()).add_values(&[1, 2, 3]);
        {
            let report = reporter.peek();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k).map(|c| c.load(Ordering::Relaxed)),
                           Some(1));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|c| c.load(Ordering::Relaxed)),
                           Some(2));
            }
            assert_eq!(report.gauges().keys().find(|k| k.name() == "brush_width"),
                       None);
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
            assert_eq!(report.stats().keys().find(|k| k.name() == "tree_len"), None);
        }

        metrics.counter("happy_accidents".into()).incr(2);
        metrics.gauge("brush_width".into()).set(5);
        metrics.stat("stroke_len".into()).add_values(&[1, 2, 3]);
        metrics.stat("tree_len".into()).add_values(&[3, 4, 5]);
        {
            let report = reporter.peek();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|g| g.load(Ordering::Relaxed)),
                           Some(3));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|g| g.load(Ordering::Relaxed)),
                           Some(2));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "brush_width")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|g| g.load(Ordering::Relaxed)),
                           Some(5));
            }
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expeced stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "tree_len")
                    .expect("expeced stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
        }
    }

    #[test]
    fn test_report_take() {
        let (metrics, mut reporter) = super::new();
        let metrics = metrics.labeled("joy".into(), "painting".into());

        metrics.counter("happy_accidents".into()).incr(1);
        metrics.gauge("paint_level".into()).set(2);
        metrics.stat("stroke_len".into()).add_values(&[1, 2, 3]);
        {
            let report = reporter.take();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k), Some(&1));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|g| g.load(Ordering::Relaxed)),
                           Some(2));
            }
            assert_eq!(report.gauges().keys().find(|k| k.name() == "brush_width"),
                       None);
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats.contains_key(&k));
            }
            assert_eq!(report.stats.keys().find(|k| k.name() == "tree_len"), None);
        }

        metrics.counter("happy_accidents".into()).incr(2);
        metrics.gauge("brush_width".into()).set(5);
        metrics.stat("tree_len".into()).add_values(&[3, 4, 5]);
        {
            let report = reporter.take();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k).map(|c| c.load(Ordering::Relaxed)),
                           Some(&3));
            }
            assert_eq!(report.gauges().keys().find(|k| k.name() == "paint_level"),
                       None);
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "brush_width")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k).map(|g| g.load(Ordering::Relaxed)),
                           Some(5));
            }
            assert_eq!(report.stats().keys().find(|k| k.name() == "stroke_len"),
                       None);
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "tree_len")
                    .expect("expeced stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
        }
    }
}
