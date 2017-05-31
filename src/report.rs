use super::{Key, Registry, CounterMap, GaugeMap};
use hdrsample::Histogram;
use ordermap::OrderMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

type ReportCounterMap = OrderMap<Key, usize>;
type ReportGaugeMap = OrderMap<Key, usize>;
type ReportStatMap = OrderMap<Key, Histogram<usize>>;

pub fn new(registry: Arc<Mutex<Registry>>) -> Reporter {
    Reporter(registry)
}

#[derive(Clone)]
pub struct Reporter(Arc<Mutex<Registry>>);

impl Reporter {
    /// Obtains a read-only view of a metrics report without clearing the underlying state.
    pub fn peek(&self) -> Report {
        let registry = self.0.lock().unwrap();
        let counters = snap_counters(&registry.counters);
        let gauges = snap_gauges(&registry.gauges);
        let stats = {
            let mut snap = ReportStatMap::with_capacity(registry.stats.len());
            for (k, v) in &registry.stats {
                let v = v.lock().unwrap();
                snap.insert(k.clone(), v.clone());
            }
            snap
        };
        Report {
            counters,
            gauges,
            stats,
        }
    }

    /// Obtains a Report and removes unused metrics.
    pub fn take(&mut self) -> Report {
        let mut registry = self.0.lock().unwrap();
        let counters = {
            let snap = snap_counters(&registry.counters);
            registry.counters.retain(|_, v| Arc::weak_count(v) > 0);
            snap
        };
        let gauges = {
            let snap = snap_gauges(&registry.gauges);
            registry.gauges.retain(|_, v| Arc::weak_count(v) > 0);
            snap
        };
        let stats = {
            let mut snap = ReportStatMap::with_capacity(registry.stats.len());
            for (k, ptr) in registry.stats.iter_mut() {
                let mut orig = ptr.lock().unwrap();
                snap.insert(k.clone(), orig.clone());
                orig.clear();
            }
            registry.stats.retain(|_, v| Arc::weak_count(v) > 0);
            snap
        };
        Report {
            counters,
            gauges,
            stats,
        }
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
