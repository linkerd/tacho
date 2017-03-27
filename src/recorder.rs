use super::{CounterKey, GaugeKey, StatKey};
use futures::sync::mpsc;
use std::collections::{HashMap, VecDeque};
use std::mem;
use twox_hash::RandomXxHashBuilder;

pub fn factory(tx: mpsc::UnboundedSender<Sample>) -> RecorderFactory {
    RecorderFactory(tx)
}

/// Produce reusable keys with the receiver's labels.  These keys are to be passed to a
/// short-lived `Recorder` in order to report metrics to the `Aggregator`.
#[derive(Clone)]
pub struct RecorderFactory(mpsc::UnboundedSender<Sample>);
impl RecorderFactory {
    pub fn mk(&self) -> Recorder {
        Recorder {
            tx: self.0.clone(),
            sample: Sample::default(),
        }
    }
}

/// Records a batch of metrics to be sent to the `Aggregator`.
///
/// No updates are sent to the `Aggregator` until the Recorder is dropped.
#[derive(Clone)]
pub struct Recorder {
    tx: mpsc::UnboundedSender<Sample>,
    sample: Sample,
}

impl Recorder {
    pub fn incr(&mut self, k: &CounterKey, n: u64) {
        if let Some(mut curr) = self.sample.counters.get_mut(k) {
            *curr += n;
            return;
        }
        self.sample.counters.insert(k.clone(), n);
    }

    pub fn set(&mut self, k: &GaugeKey, n: u64) {
        if let Some(mut curr) = self.sample.gauges.get_mut(k) {
            *curr = n;
            return;
        }
        self.sample.gauges.insert(k.clone(), n);
    }

    pub fn add(&mut self, k: &StatKey, n: u64) {
        if let Some(mut curr) = self.sample.stats.get_mut(k) {
            curr.push_back(n);
            return;
        }

        let mut vals = VecDeque::new();
        vals.push_back(n);
        self.sample.stats.insert(k.clone(), vals);
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        // Steal the sample from the recorder so we can give it to the channel without
        // copying.
        let sample = mem::replace(&mut self.sample, Sample::default());
        if mpsc::UnboundedSender::send(&self.tx, sample).is_err() {
            info!("dropping metrics");
        }
    }
}

/// Stores the results from a a `Record` instance.
#[derive(Clone, Debug)]
pub struct Sample {
    pub counters: HashMap<CounterKey, u64, RandomXxHashBuilder>,
    pub gauges: HashMap<GaugeKey, u64, RandomXxHashBuilder>,
    pub stats: HashMap<StatKey, VecDeque<u64>, RandomXxHashBuilder>,
}
impl Default for Sample {
    fn default() -> Sample {
        Sample {
            counters: HashMap::default(),
            gauges: HashMap::default(),
            stats: HashMap::default(),
        }
    }
}
