use super::{Key, HistogramWithSum, Registry, CounterMap, GaugeMap, StatMap};
use ordermap::OrderMap;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::Ordering;

type ReportCounterMap = OrderMap<Key, usize>;
type ReportGaugeMap = OrderMap<Key, usize>;
type ReportStatMap = OrderMap<Key, HistogramWithSum>;

pub fn new(registry: Arc<Mutex<Registry>>) -> Reporter {
    Reporter(registry)
}

#[derive(Clone)]
pub struct Reporter(Arc<Mutex<Registry>>);

impl Reporter {
    /// Obtains a read-only view of a metrics report without clearing the underlying state.
    pub fn peek(&self) -> Report {
        let registry = self.0.lock();
        Report {
            counters: snap_counters(&registry.counters),
            gauges: snap_gauges(&registry.gauges),
            stats: snap_stats(&registry.stats, false),
        }
    }

    /// Obtains a Report and removes unused metrics.
    pub fn take(&mut self) -> Report {
        let mut registry = self.0.lock();

        let report = Report {
            counters: snap_counters(&registry.counters),
            gauges: snap_gauges(&registry.gauges),
            stats: snap_stats(&registry.stats, true),
        };

        // Drop unreferenced metrics.
        registry.counters.retain(|_, v| Arc::strong_count(v) > 1);
        registry.gauges.retain(|_, v| Arc::strong_count(v) > 1);
        registry.stats.retain(|_, v| Arc::strong_count(v) > 1);

        report
    }
}

fn snap_counters(counters: &CounterMap) -> ReportCounterMap {
    let mut snap = ReportCounterMap::with_capacity(counters.len());
    for (k, v) in &*counters {
        let v = v.load(Ordering::Acquire);
        snap.insert(k.clone(), v);
    }
    snap
}

fn snap_gauges(gauges: &GaugeMap) -> ReportGaugeMap {
    let mut snap = ReportGaugeMap::with_capacity(gauges.len());
    for (k, v) in &*gauges {
        let v = v.load(Ordering::Acquire);
        snap.insert(k.clone(), v);
    }
    snap
}

fn snap_stats(stats: &StatMap, clear: bool) -> ReportStatMap {
    let mut snap = ReportStatMap::with_capacity(stats.len());
    for (k, ptr) in &*stats {
        let mut orig = ptr.lock();
        snap.insert(k.clone(), orig.clone());
        if clear {
            orig.clear();
        }
    }
    snap
}

pub struct Report {
    counters: ReportCounterMap,
    gauges: ReportGaugeMap,
    stats: ReportStatMap,
}
impl Report {
    pub fn counters(&self) -> &ReportCounterMap {
        &self.counters
    }
    pub fn gauges(&self) -> &ReportGaugeMap {
        &self.gauges
    }
    pub fn stats(&self) -> &ReportStatMap {
        &self.stats
    }
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.stats.is_empty()
    }
    pub fn len(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.stats.len()
    }
}
