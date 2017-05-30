//! A thread-safe, `Future`-aware metrics library.
//!
//! Many programs need to information about runtime performance: the number of requests
//! served, a distribution of request latency, the number of failures, the number of loop
//! iterations, etc. `tacho::new` creates a shareable, scopable metrics registry and a
//! `Reporter`. The `Scope` supports the creation of `Counter`, `Gauge`, and `Stat`
//! handles that may be used to report values. Each of these receivers maintains a weak
//! reference back to the central stats registry.
//!
//! ## Performance
//!
//! Labels are stored in a `BTreeMap` because they are used as hash keys and, therefore,
//! need to implement `Hash`.

// For benchmarks.
#![feature(test)]

// For benchmarks.
#![feature(test)]

extern crate hdrsample;
#[macro_use]
extern crate log;
extern crate ordermap;
extern crate test;

use hdrsample::Histogram;
use ordermap::OrderMap;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod prometheus;
mod report;
mod timing;

pub use report::{Reporter, Report};
pub use timing::Timing;

type Labels = BTreeMap<&'static str, String>;
type CounterMap = OrderMap<Key, Arc<AtomicUsize>>;
type GaugeMap = OrderMap<Key, Arc<AtomicUsize>>;
type StatMap = OrderMap<Key, Arc<Mutex<Histogram<usize>>>>;

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
        registry: registry.clone(),
    };

    (scope, report::new(registry))
}

/// Describes a metric.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    name: &'static str,
    labels: Labels,
}
impl Key {
    fn new(name: &'static str, labels: Labels) -> Key {
        Key {
            name: name,
            labels: labels,
        }
    }

    pub fn name(&self) -> &str {
        self.name
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
    registry: Arc<Mutex<Registry>>,
}

impl Scope {
    /// Accesses scoping labels.
    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    /// Adds a label into scope (potentially overwriting).
    pub fn labeled(mut self, k: &'static str, v: String) -> Scope {
        self.labels.insert(k, v);
        self
    }

    /// Creates a Counter with the given name.
    pub fn counter(&self, name: &'static str) -> Counter {
        let key = Key::new(name, self.labels.clone());
        let mut reg = self.registry
            .lock()
            .expect("failed to obtain lock on registry");

        if let Some(c) = reg.counters.get(&key) {
            return Counter(Arc::downgrade(c));
        }

        let c = Arc::new(AtomicUsize::default());
        let counter = Counter(Arc::downgrade(&c));
        reg.counters.insert(key, c);
        counter
    }

    /// Creates a Gauge with the given name.
    pub fn gauge(&self, name: &'static str) -> Gauge {
        let key = Key::new(name, self.labels.clone());
        let mut reg = self.registry
            .lock()
            .expect("failed to obtain lock on registry");

        if let Some(g) = reg.gauges.get(&key) {
            return Gauge(Arc::downgrade(g));
        }

        let g = Arc::new(AtomicUsize::default());
        let gauge = Gauge(Arc::downgrade(&g));
        reg.gauges.insert(key, g);
        gauge
    }

    /// Creates a Stat with the given name.
    ///
    /// The underlying histogram is automatically resized as values are added.
    pub fn stat(&self, name: &'static str) -> Stat {
        let key = Key::new(name, self.labels.clone());
        self.mk_stat(key, None)
    }

    /// Creates a Stat with the given name and histogram paramters.
    pub fn stat_with_bounds(&self, name: &'static str, low: u64, high: u64) -> Stat {
        let key = Key::new(name, self.labels.clone());
        self.mk_stat(key, Some((low, high)))
    }

    fn mk_stat(&self, key: Key, bounds: Option<(u64, u64)>) -> Stat {
        let mut reg = self.registry
            .lock()
            .expect("failed to obtain lock on registry");

        if let Some(h) = reg.stats.get(&key) {
            let histo = Arc::downgrade(h);
            return Stat { histo, bounds };
        }

        let histo = match bounds {
            None => {
                Histogram::<usize>::new(HISTOGRAM_PRECISION).expect("failed to build Histogram")
            }
            Some((low, high)) => {
                Histogram::<usize>::new_with_bounds(low, high, HISTOGRAM_PRECISION)
                    .expect("failed to build Histogram")
            }
        };
        let s = Arc::new(Mutex::new(histo));
        let stat = Stat {
            histo: Arc::downgrade(&s),
            bounds,
        };
        reg.stats.insert(key, s);
        stat
    }
}

/// Counts monotically.
#[derive(Clone)]
pub struct Counter(Weak<AtomicUsize>);
impl Counter {
    pub fn incr(&self, v: usize) {
        if let Some(c) = self.0.upgrade() {
            c.fetch_add(v, Ordering::AcqRel);
        }
    }
}

/// Captures an instantaneous value.
#[derive(Clone)]
pub struct Gauge(Weak<AtomicUsize>);
impl Gauge {
    pub fn incr(&self, v: usize) {
        if let Some(g) = self.0.upgrade() {
            g.fetch_add(v, Ordering::AcqRel);
        }
    }
    pub fn decr(&self, v: usize) {
        if let Some(g) = self.0.upgrade() {
            g.fetch_sub(v, Ordering::AcqRel);
        }
    }
    pub fn set(&self, v: usize) {
        if let Some(g) = self.0.upgrade() {
            g.store(v, Ordering::Release);
        }
    }
}

/// Caputres a distribution of values.
#[derive(Clone)]
pub struct Stat {
    histo: Weak<Mutex<Histogram<usize>>>,
    bounds: Option<(u64, u64)>,
}

const HISTOGRAM_PRECISION: u32 = 4;

impl Stat {
    pub fn add(&self, v: u64) {
        self.add_values(&[v]);
    }

    pub fn add_values(&self, vs: &[u64]) {
        if let Some(h) = self.histo.upgrade() {
            let mut histo = h.lock().expect("failed to obtain lock for stat");
            for v in vs {
                if let Err(e) = histo.record(*v) {
                    error!("failed to add value to histogram: {:?}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;

    static DEFAULT_METRIC_NAME: &'static str = "a_sufficiently_long_name";

    #[bench]
    fn bench_scope_clone(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { let _ = metrics.clone(); });
    }

    #[bench]
    fn bench_scope_label(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { let _ = metrics.clone().labeled("foo", "bar".into()); });
    }

    #[bench]
    fn bench_scope_clone_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_scope_clone_x1000");
        b.iter(move || for scope in &scopes {
                   let _ = scope.clone();
               });
    }

    #[bench]
    fn bench_scope_label_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_scope_label_x1000");
        b.iter(move || for scope in &scopes {
                   let _ = scope.clone().labeled("foo", "bar".into());
               });
    }

    #[bench]
    fn bench_counter_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { let _ = metrics.counter(DEFAULT_METRIC_NAME); });
    }

    #[bench]
    fn bench_gauge_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { let _ = metrics.gauge(DEFAULT_METRIC_NAME); });
    }

    #[bench]
    fn bench_stat_create(b: &mut Bencher) {
        let (metrics, _) = super::new();
        b.iter(move || { let _ = metrics.stat(DEFAULT_METRIC_NAME); });
    }

    #[bench]
    fn bench_counter_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_counter_create_x1000");
        b.iter(move || for scope in &scopes {
                   scope.counter(DEFAULT_METRIC_NAME);
               });
    }

    #[bench]
    fn bench_gauge_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_gauge_create_x1000");
        b.iter(move || for scope in &scopes {
                   scope.gauge(DEFAULT_METRIC_NAME);
               });
    }

    #[bench]
    fn bench_stat_create_x1000(b: &mut Bencher) {
        let scopes = mk_scopes(1000, "bench_stat_create_x1000");
        b.iter(move || for scope in &scopes {
                   scope.stat(DEFAULT_METRIC_NAME);
               });
    }

    #[bench]
    fn bench_counter_update(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let c = metrics.counter(DEFAULT_METRIC_NAME);
        b.iter(move || c.incr(1));
    }

    #[bench]
    fn bench_gauge_update(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let g = metrics.gauge(DEFAULT_METRIC_NAME);
        b.iter(move || g.set(1));
    }

    #[bench]
    fn bench_stat_update(b: &mut Bencher) {
        let (metrics, _) = super::new();
        let s = metrics.stat(DEFAULT_METRIC_NAME);
        b.iter(move || s.add(1));
    }

    #[bench]
    fn bench_counter_update_x1000(b: &mut Bencher) {
        let counters: Vec<Counter> = mk_scopes(1000, "bench_counter_update_x1000")
            .iter()
            .map(|s| s.counter(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || for c in &counters {
                   c.incr(1)
               });
    }

    #[bench]
    fn bench_gauge_update_x1000(b: &mut Bencher) {
        let gauges: Vec<Gauge> = mk_scopes(1000, "bench_gauge_update_x1000")
            .iter()
            .map(|s| s.gauge(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || for g in &gauges {
                   g.set(1)
               });
    }

    #[bench]
    fn bench_stat_update_x1000(b: &mut Bencher) {
        let stats: Vec<Stat> = mk_scopes(1000, "bench_stat_update_x1000")
            .iter()
            .map(|s| s.stat(DEFAULT_METRIC_NAME))
            .collect();
        b.iter(move || for s in &stats {
                   s.add(1)
               });
    }

    #[bench]
    fn bench_stat_add_x1000(b: &mut Bencher) {
        let s = {
            let (metrics, _) = super::new();
            metrics.stat(DEFAULT_METRIC_NAME)
        };
        b.iter(move || for i in 0..1000 {
                   s.add(i)
               });
    }

    fn mk_scopes(n: usize, name: &str) -> Vec<Scope> {
        let (metrics, _) = super::new();
        let metrics = metrics
            .labeled("test_name", name.into())
            .labeled("total_iterations", format!("{}", n));
        (0..n)
            .map(|i| metrics.clone().labeled("iteration", format!("{}", i)))
            .collect()
    }

    #[test]
    fn test_report_peek() {
        let (metrics, reporter) = super::new();
        let metrics = metrics.labeled("joy", "painting".into());

        let happy_accidents = metrics.counter("happy_accidents");
        let paint_level = metrics.gauge("paint_level");
        let stroke_len = metrics.stat("stroke_len");

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
            assert_eq!(report.gauges().keys().find(|k| k.name() == "brush_width"),
                       None);
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
        let tree_len = metrics.stat("tree_len");

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
        let metrics = metrics.labeled("joy", "painting".into());

        let happy_accidents = metrics.counter("happy_accidents");
        let paint_level = metrics.gauge("paint_level");
        let stroke_len = metrics.stat("stroke_len");
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
        let tree_len = metrics.stat("tree_len");
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
            assert_eq!(report.gauges().keys().find(|k| k.name() == "paint_level"),
                       None);
            {
                let k = report
                    .gauges()
                    .keys()
                    .find(|k| k.name() == "brush_width")
                    .expect("expected gauge");
                assert_eq!(k.labels.get("joy"), Some(&"painting".to_string()));
                assert_eq!(report.gauges().get(&k), Some(&5));
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
