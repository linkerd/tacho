use std::collections::BTreeMap;
use std::sync::Arc;

pub trait MetricKey {
    fn name(&self) -> &str;
    fn labels(&self) -> &BTreeMap<String, String>;
}

/// The internal representation of a `MetricKey`, common to all key types.
///
/// A `BTreeMap` is used so that the key is hashable.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct InnerKey {
    name: String,
    labels: BTreeMap<String, String>,
}
impl InnerKey {
    /// Constructs a reference-counted key.
    ///
    /// This is thread-safe and reference-counted so that keys may be cloned (namely, when
    /// initializing a map entry) without copying the entire set of labels.
    fn arc(name: String, labels: BTreeMap<String, String>) -> Arc<InnerKey> {
        Arc::new(InnerKey {
            name: name,
            labels: labels,
        })
    }
}
impl MetricKey for InnerKey {
    fn name(&self) -> &str {
        &self.name
    }
    fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CounterKey(Arc<InnerKey>);
impl CounterKey {
    pub fn new(name: String, labels: BTreeMap<String, String>) -> CounterKey {
        CounterKey(InnerKey::arc(name, labels))
    }
}
impl MetricKey for CounterKey {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn labels(&self) -> &BTreeMap<String, String> {
        self.0.labels()
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct GaugeKey(Arc<InnerKey>);
impl GaugeKey {
    pub fn new(name: String, labels: BTreeMap<String, String>) -> GaugeKey {
        GaugeKey(InnerKey::arc(name, labels))
    }
}
impl MetricKey for GaugeKey {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn labels(&self) -> &BTreeMap<String, String> {
        self.0.labels()
    }
}

/// Describes a key whose values will be histogramed.
///
/// `StatKey`s include the default histogram parameters (`low`, `high). Histograms are,
/// however, resized as needed.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatKey {
    key: Arc<InnerKey>,
    low: u64,
    high: u64,
}
impl StatKey {
    pub fn new(name: String, labels: BTreeMap<String, String>, low: u64, high: u64) -> StatKey {
        assert!(low >= 1);
        assert!(high >= 2 * low);
        StatKey {
            key: InnerKey::arc(name, labels),
            low: low,
            high: high,
        }
    }

    pub fn low(&self) -> u64 {
        self.low
    }
    pub fn high(&self) -> u64 {
        self.high
    }
}
impl MetricKey for StatKey {
    fn name(&self) -> &str {
        self.key.name()
    }
    fn labels(&self) -> &BTreeMap<String, String> {
        self.key.labels()
    }
}
