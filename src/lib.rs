//! A thread-safe, `Future`-aware metrics library.
//!
//! Many programs need to information about runtime performance: the number of requests
//! served, a distribution of request latency, the number of failures, the number of loop
//! iterations, etc. `tacho::new` creates a shareable, scopable metrics registry and a
//! `Reporter`. The `Scope` supports the creation of `Counter`, `Gauge`, and `Stat`
//! handles that may be used to report values. Each of these receivers maintains a
//! reference back to the central stats registry.
//!
//! ## Performance
//!
//! Labels are stored in a `BTreeMap` because they are used as hash keys and, therefore,
//! need to implement `Hash`.


#![cfg_attr(test, feature(test))]

extern crate futures;
extern crate hdrsample;
#[macro_use]
extern crate log;
extern crate ordermap;
extern crate parking_lot;
#[cfg(test)]
extern crate test;

use futures::{Future, Poll};
use hdrsample::Histogram;
use ordermap::OrderMap;
use parking_lot::Mutex;
use std::boxed::Box;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

pub mod prometheus;
mod report;
mod timing;

pub use report::{Reporter, Report};
pub use timing::Timing;

type Labels = BTreeMap<&'static str, String>;
type CounterMap = OrderMap<Key, Arc<AtomicUsize>>;
type GaugeMap = OrderMap<Key, Arc<AtomicUsize>>;
type StatMap = OrderMap<Key, Arc<Mutex<HistogramWithSum>>>;

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum Prefix {
    Root,
    Node {
        prefix: Arc<Prefix>,
        value: &'static str,
    },
}

/// Creates a metrics registry.
///
/// The returned `Scope` may be you used to instantiate metrics. Labels may be attached to
/// the scope so that all metrics created by this `Scope` are annotated.
///
/// The returned `Reporter` supports consumption of metrics values.
pub fn new() -> (Scope, Reporter) {
    let registry = Arc::new(Mutex::new(Registry::default()));

    let scope = Scope {
        labels: Labels::default(),
        prefix: Arc::new(Prefix::Root),
        registry: registry.clone(),
    };

    (scope, report::new(registry))
}

/// Describes a metric.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    name: &'static str,
    prefix: Arc<Prefix>,
    labels: Labels,
}
impl Key {
    fn new(name: &'static str, prefix: Arc<Prefix>, labels: Labels) -> Key {
        Key {
            name,
            prefix,
            labels,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn prefix(&self) -> &Arc<Prefix> {
        &self.prefix
    }
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

#[derive(Default)]
pub struct Registry {
    counters: CounterMap,
    gauges: GaugeMap,
    stats: StatMap,
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
    prefix: Arc<Prefix>,
    registry: Arc<Mutex<Registry>>,
}

impl Scope {
    /// Accesses scoping labels.
    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    /// Adds a label into scope (potentially overwriting).
    pub fn labeled<D: fmt::Display>(mut self, k: &'static str, v: D) -> Self {
        self.labels.insert(k, format!("{}", v));
        self
    }

    /// Appends a prefix to the current scope.
    pub fn prefixed(mut self, value: &'static str) -> Self {
        let p = Prefix::Node {
            prefix: self.prefix,
            value,
        };
        self.prefix = Arc::new(p);
        self
    }

    /// Creates a Counter with the given name.
    pub fn counter(&self, name: &'static str) -> Counter {
        let key = Key::new(name, self.prefix.clone(), self.labels.clone());
        let mut reg = self.registry.lock();

        if let Some(c) = reg.counters.get(&key) {
            return Counter(c.clone());
        }

        let c = Arc::new(AtomicUsize::new(0));
        let counter = Counter(c.clone());
        reg.counters.insert(key, c);
        counter
    }

    /// Creates a Gauge with the given name.
    pub fn gauge(&self, name: &'static str) -> Gauge {
        let key = Key::new(name, self.prefix.clone(), self.labels.clone());
        let mut reg = self.registry.lock();

        if let Some(g) = reg.gauges.get(&key) {
            return Gauge(g.clone());
        }

        let g = Arc::new(AtomicUsize::new(0));
        let gauge = Gauge(g.clone());
        reg.gauges.insert(key, g);
        gauge
    }

    /// Creates a Stat with the given name.
    ///
    /// The underlying histogram is automatically resized as values are added.
    pub fn stat(&self, name: &'static str) -> Stat {
        let key = Key::new(name, self.prefix.clone(), self.labels.clone());
        self.mk_stat(key, None)
    }

    pub fn timer_us(&self, name: &'static str) -> Timer {
        Timer {
            stat: self.stat(name),
            unit: TimeUnit::Micros,
        }
    }

    pub fn timer_ms(&self, name: &'static str) -> Timer {
        Timer {
            stat: self.stat(name),
            unit: TimeUnit::Millis,
        }
    }

    /// Creates a Stat with the given name and histogram paramters.
    pub fn stat_with_bounds(&self, name: &'static str, low: u64, high: u64) -> Stat {
        let key = Key::new(name, self.prefix.clone(), self.labels.clone());
        self.mk_stat(key, Some((low, high)))
    }

    fn mk_stat(&self, key: Key, bounds: Option<(u64, u64)>) -> Stat {
        let mut reg = self.registry.lock();

        if let Some(h) = reg.stats.get(&key) {
            return Stat { histo: h.clone(), bounds };
        }

        let histo = Arc::new(Mutex::new(HistogramWithSum::new(bounds)));
        reg.stats.insert(key, histo.clone());
        Stat { histo, bounds }
    }
}

/// Counts monotically.
#[derive(Clone)]
pub struct Counter(Arc<AtomicUsize>);
impl Counter {
    pub fn incr(&self, v: usize) {
        self.0.fetch_add(v, Ordering::AcqRel);
    }
}

/// Captures an instantaneous value.
#[derive(Clone)]
pub struct Gauge(Arc<AtomicUsize>);
impl Gauge {
    pub fn incr(&self, v: usize) {
        self.0.fetch_add(v, Ordering::AcqRel);
    }
    pub fn decr(&self, v: usize) {
        self.0.fetch_sub(v, Ordering::AcqRel);
    }
    pub fn set(&self, v: usize) {
        self.0.store(v, Ordering::Release);
    }
}

/// Histograms hold up to 4 significant figures.
const HISTOGRAM_PRECISION: u32 = 4;

/// Tracks a distribution of values with their sum.
///
/// `hdrsample::Histogram` does not track a sum by default; but prometheus expects a `sum`
/// for histograms.
#[derive(Clone)]
pub struct HistogramWithSum {
    histogram: Histogram<usize>,
    sum: u64,
}

impl HistogramWithSum {
    /// Constructs a new `HistogramWithSum`, possibly with bounds.
    fn new(bounds: Option<(u64, u64)>) -> Self {
        let h = match bounds {
            None => Histogram::<usize>::new(HISTOGRAM_PRECISION),
            Some((l, h)) => Histogram::<usize>::new_with_bounds(l, h, HISTOGRAM_PRECISION),
        };
        let histogram = h.expect("failed to create histogram");
        HistogramWithSum { histogram, sum: 0 }
    }

    /// Record a value to
    fn record(&mut self, v: u64) {
        if let Err(e) = self.histogram.record(v) {
            error!("failed to add value to histogram: {:?}", e);
        }
        if v >= ::std::u64::MAX - self.sum {
            self.sum = ::std::u64::MAX
        } else {
            self.sum += v;
        }
    }

    pub fn histogram(&self) -> &Histogram<usize> {
        &self.histogram
    }
    pub fn count(&self) -> u64 {
        self.histogram.count()
    }
    pub fn max(&self) -> u64 {
        self.histogram.max()
    }
    pub fn min(&self) -> u64 {
        self.histogram.min()
    }
    pub fn sum(&self) -> u64 {
        self.sum
    }

    pub fn clear(&mut self) {
        self.histogram.reset();
        self.sum = 0;
    }
}

/// Captures a distribution of values.
#[derive(Clone)]
pub struct Stat {
    histo: Arc<Mutex<HistogramWithSum>>,
    bounds: Option<(u64, u64)>,
}

impl Stat {
    pub fn add(&self, v: u64) {
        let mut histo = self.histo.lock();
        histo.record(v);
    }

    pub fn add_values(&mut self, vs: &[u64]) {
        let mut histo = self.histo.lock();
        for v in vs {
            histo.record(*v)
        }
    }
}

#[derive(Clone)]
pub struct Timer {
    stat: Stat,
    unit: TimeUnit,
}
#[derive(Copy, Clone)]
pub enum TimeUnit {
    Millis,
    Micros,
}
impl Timer {
    pub fn record_since(&self, t0: Instant) {
        self.stat.add(to_u64(t0, self.unit));
    }

    pub fn time<F>(&self, fut: F) -> Timed<F>
    where
        F: Future + 'static,
    {
        let stat = self.stat.clone();
        let unit = self.unit;
        let f = futures::lazy(move || {
            // Start timing once the future is actually being invoked (and not
            // when the object is created).
            let t0 = Timing::start();
            fut.then(move |v| {
                stat.add(to_u64(t0, unit));
                v
            })
        });
        Timed(Box::new(f))
    }
}

fn to_u64(t0: Instant, unit: TimeUnit) -> u64 {
    match unit {
        TimeUnit::Millis => t0.elapsed_ms(),
        TimeUnit::Micros => t0.elapsed_us(),
    }
}

pub struct Timed<F: Future>(Box<Future<Item = F::Item, Error = F::Error>>);
impl<F: Future> Future for Timed<F> {
    type Item = F::Item;
    type Error = F::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::{Bencher, black_box};

    static DEFAULT_METRIC_NAME: &'static str = "a_sufficiently_long_name";

    #[bench]
    fn bench_scope_clone(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || black_box(metrics.clone()));
    }

    #[bench]
    fn bench_scope_label(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { black_box(metrics.clone().labeled("foo", "bar")) });
    }

    #[bench]
    fn bench_scope_clone_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_scope_clone_x1000");
        b.iter(move || for scope in &scopes {
            black_box(scope.clone());
        });
    }

    #[bench]
    fn bench_scope_label_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_scope_label_x1000");
        b.iter(move || for scope in &scopes {
            black_box(scope.clone().labeled("foo", "bar"));
        });
    }

    #[bench]
    fn bench_counter_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || black_box(metrics.counter(DEFAULT_METRIC_NAME)));
    }

    #[bench]
    fn bench_gauge_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || black_box(metrics.gauge(DEFAULT_METRIC_NAME)));
    }

    #[bench]
    fn bench_stat_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || black_box(metrics.stat(DEFAULT_METRIC_NAME)));
    }

    #[bench]
    fn bench_counter_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_counter_create_x1000");
        b.iter(move || for scope in &scopes {
            black_box(scope.counter(DEFAULT_METRIC_NAME));
        });
    }

    #[bench]
    fn bench_gauge_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_gauge_create_x1000");
        b.iter(move || for scope in &scopes {
            black_box(scope.gauge(DEFAULT_METRIC_NAME));
        });
    }

    #[bench]
    fn bench_stat_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_stat_create_x1000");
        b.iter(move || for scope in &scopes {
            black_box(scope.stat(DEFAULT_METRIC_NAME));
        });
    }

    #[bench]
    fn bench_counter_update(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let c = metrics.counter(DEFAULT_METRIC_NAME);
        b.iter(move || {
            c.incr(1);
            black_box(&c);
        });
    }

    #[bench]
    fn bench_gauge_update(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let g = metrics.gauge(DEFAULT_METRIC_NAME);
        b.iter(move || {
            g.set(1);
            black_box(&g);
        });
    }

    #[bench]
    fn bench_stat_update(b: &mut Bencher) {
        let (scope, _) = super::new();
        let s = scope.stat(DEFAULT_METRIC_NAME);
        b.iter(move || {
            s.add(1);
            black_box(&s);
        });
    }

    #[bench]
    fn bench_counter_update_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_counter_update_x1000");
        let counters: Vec<Counter> = scopes
            .iter()
            .map(|s| s.counter(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || {
            for c in &counters {
                c.incr(1);
            }
            black_box(&counters);
        });
    }

    #[bench]
    fn bench_gauge_update_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_gauge_update_x1000");
        let gauges: Vec<Gauge> = scopes
            .iter()
            .map(|s| s.gauge(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || {
            for g in &gauges {
                g.set(1);
            }
            black_box(&gauges);
        });
    }

    #[bench]
    fn bench_stat_update_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_stat_update_x1000");
        let stats: Vec<Stat> = scopes
            .iter()
            .map(|s| s.stat(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || {
            for s in &stats {
                s.add(1)
            }
            black_box(&stats);
        });
    }

    #[bench]
    fn bench_stat_add_x1000(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let s = metrics.stat(DEFAULT_METRIC_NAME);
        b.iter(move || {
            for i in 0..1000 {
                s.add(i);
            }
            black_box(&s);
        });
    }

    fn mk_scopes(n: usize, name: &str) -> Vec<Scope> {
        let (metrics, _) = super::new();
        let metrics = metrics.prefixed("t").labeled("test_name", name).labeled(
            "total_iterations",
            n,
        );
        (0..n)
            .map(|i| metrics.clone().labeled("iteration", format!("{}", i)))
            .collect()
    }

    #[test]
    fn test_report_peek() {
        let (metrics, reporter) = super::new();
        let metrics = metrics.labeled("joy", "painting");

        let happy_accidents = metrics.counter("happy_accidents");
        let paint_level = metrics.gauge("paint_level");
        let mut stroke_len = metrics.stat("stroke_len");

        happy_accidents.incr(1);
        paint_level.set(2);
        stroke_len.add_values(&[1, 2, 3]);

        {
            let report = reporter.peek();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter: happy_accidents");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k), Some(&1));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge: paint_level");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&2));
            }
            assert_eq!(
                report.gauges().keys().find(|k| k.name() == "brush_width"),
                None
            );
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat: stroke_len");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
            assert_eq!(report.stats().keys().find(|k| k.name() == "tree_len"), None);
        }

        drop(paint_level);
        let brush_width = metrics.gauge("brush_width");
        let mut tree_len = metrics.stat("tree_len");

        happy_accidents.incr(2);
        brush_width.set(5);
        stroke_len.add_values(&[1, 2, 3]);
        tree_len.add_values(&[3, 4, 5]);

        {
            let report = reporter.peek();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter: happy_accidents");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k), Some(&3));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge: paint_level");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&2));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "brush_width")
                    .expect("expected gauge: brush_width");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&5));
            }
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat: stroke_len");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "tree_len")
                    .expect("expected stat: tree_len");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
        }
    }

    #[test]
    fn test_report_take() {
        let (metrics, mut reporter) = super::new();
        let metrics = metrics.labeled("joy", "painting");

        let happy_accidents = metrics.counter("happy_accidents");
        let paint_level = metrics.gauge("paint_level");
        let mut stroke_len = metrics.stat("stroke_len");
        happy_accidents.incr(1);
        paint_level.set(2);
        stroke_len.add_values(&[1, 2, 3]);
        {
            let report = reporter.take();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter: happy_accidents");
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
                assert_eq!(report.gauges().get(&k), Some(&2));
            }
            assert_eq!(
                report.gauges().keys().find(|k| k.name() == "brush_width"),
                None
            );
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
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
        }

        drop(paint_level);
        drop(stroke_len);
        {
            let report = reporter.take();
            {
                let counters = report.counters();
                let k = counters
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter: happy_accidents");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(counters.get(&k), Some(&1));
            }
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "paint_level")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&2));
            }
            {
                let k = report
                    .stats()
                    .keys()
                    .find(|k| k.name() == "stroke_len")
                    .expect("expected stat");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert!(report.stats().contains_key(&k));
            }
        }

        let brush_width = metrics.gauge("brush_width");
        let mut tree_len = metrics.stat("tree_len");
        happy_accidents.incr(2);
        brush_width.set(5);
        tree_len.add_values(&[3, 4, 5]);
        {
            let report = reporter.take();
            {
                let k = report
                    .counters()
                    .keys()
                    .find(|k| k.name() == "happy_accidents")
                    .expect("expected counter: happy_accidents");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.counters().get(&k), Some(&3));
            }
            assert_eq!(
                report.gauges().keys().find(|k| k.name() == "paint_level"),
                None
            );
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "brush_width")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&5));
            }
            assert_eq!(
                report.stats().keys().find(|k| k.name() == "stroke_len"),
                None
            );
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
