/// 10s in ms, or 10ms in us.
const DEFAULT_TIMING_MAX: u64 = 10_000;
use super::{CounterKey, GaugeKey, StatKey};
use std::collections::BTreeMap;

/// Scopes stats production.
///
/// Applications may clone and attach labels to the receiver so that keys produced from
/// this receiver have the proper attributions (without pushing this logic into the
/// application).
#[derive(Clone)]
pub struct Scope(BTreeMap<String, String>);
impl Default for Scope {
    fn default() -> Scope {
        Scope(BTreeMap::default())
    }
}
impl Scope {
    pub fn new(labels: BTreeMap<String, String>) -> Scope {
        Scope(labels)
    }

    /// Places the current MetricsBundle into the tx receiver for later processing.
    pub fn labeled(self, k: String, v: String) -> Scope {
        let mut labels = self.0;
        labels.insert(k, v);
        Scope(labels)
    }

    pub fn counter(&self, name: String) -> CounterKey {
        CounterKey::new(name, self.0.clone())
    }

    pub fn gauge(&self, name: String) -> GaugeKey {
        GaugeKey::new(name, self.0.clone())
    }

    // TODO should this include histogram info?
    pub fn stat_with_hint(&self, name: String, low: u64, high: u64) -> StatKey {
        StatKey::new(name, self.0.clone(), low, high)
    }

    pub fn stat(&self, name: String) -> StatKey {
        // Should be expanded automatically.
        self.stat_with_hint(name, 1, 2)
    }

    pub fn timing_ms(&self, name: String) -> StatKey {
        self.stat_with_hint(name, 1, DEFAULT_TIMING_MAX)
    }

    pub fn timing_us(&self, name: String) -> StatKey {
        self.stat_with_hint(name, 1, DEFAULT_TIMING_MAX)
    }
}
