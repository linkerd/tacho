use super::{CounterMap, GaugeMap, StatMap};
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub fn new(counters: Arc<RwLock<CounterMap>>,
           gauges: Arc<RwLock<GaugeMap>>,
           stats: Arc<RwLock<StatMap>>)
           -> Reporter {
    Reporter {
        counters: counters,
        gauges: gauges,
        stats: stats,
    }
}

#[derive(Clone)]
pub struct Reporter {
    counters: Arc<RwLock<CounterMap>>,
    gauges: Arc<RwLock<GaugeMap>>,
    stats: Arc<RwLock<StatMap>>,
}

impl Reporter {
    /// Obtains a read-only view of a metrics report without clearing the underlying state.
    pub fn peek(&self) -> ReportPeek {
        let counters = self.counters
            .read()
            .expect("failed to obtain read lock for counters");
        let gauges = self.gauges
            .read()
            .expect("failed to obtain read lock for gauges");
        let stats = self.stats
            .read()
            .expect("failed to obtain read lock for stats");
        ReportPeek {
            counters: counters,
            gauges: gauges,
            stats: stats,
        }
    }

    /// Obtains a Report and clears the underlying gauges and stats.
    ///
    /// Counters are copied and not cleared because counters are absolute and increasing.
    pub fn take(&mut self) -> ReportTake {
        // Copy counters.
        let counters: CounterMap = {
            let orig = self.counters
                .read()
                .expect("failed to obtain write lock for counters");
            let mut snap = CounterMap::default();
            for (k, v) in orig.iter() {
                snap.insert(k.clone(), *v);
            }
            snap
        };

        // Reset gauges.
        let gauges = {
            let mut orig = self.gauges
                .write()
                .expect("failed to obtain write lock for gauges");
            let mut snap = GaugeMap::default();
            for (k, v) in orig.drain() {
                snap.insert(k, v);
            }
            snap
        };

        // Reset stats.
        let stats = {
            let mut orig = self.stats
                .write()
                .expect("failed to obtain write lock for stats");
            let mut snap = StatMap::default();
            for (k, v) in orig.drain() {
                snap.insert(k, v);
            }
            snap
        };

        ReportTake {
            counters: counters,
            gauges: gauges,
            stats: stats,
        }
    }
}

pub trait Report {
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    fn counters(&self) -> &CounterMap;
    fn gauges(&self) -> &GaugeMap;
    fn stats(&self) -> &StatMap;
}

pub struct ReportPeek<'a> {
    counters: RwLockReadGuard<'a, CounterMap>,
    gauges: RwLockReadGuard<'a, GaugeMap>,
    stats: RwLockReadGuard<'a, StatMap>,
}
impl<'a> Report for ReportPeek<'a> {
    fn counters(&self) -> &CounterMap {
        &self.counters
    }
    fn gauges(&self) -> &GaugeMap {
        &self.gauges
    }
    fn stats(&self) -> &StatMap {
        &self.stats
    }
    fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.stats.is_empty()
    }
    fn len(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.stats.len()
    }
}

pub struct ReportTake {
    pub counters: CounterMap,
    pub gauges: GaugeMap,
    pub stats: StatMap,
}
impl Report for ReportTake {
    fn counters(&self) -> &CounterMap {
        &self.counters
    }
    fn gauges(&self) -> &GaugeMap {
        &self.gauges
    }
    fn stats(&self) -> &StatMap {
        &self.stats
    }
    fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.stats.is_empty()
    }
    fn len(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.stats.len()
    }
}
