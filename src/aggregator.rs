use super::{Sample, CounterKey, GaugeKey, StatKey};
use futures::{Async, Poll, Future, Stream, task};
use futures::sync::{BiLock, mpsc};
use hdrsample::Histogram;
use std::collections::{HashMap, VecDeque};
use twox_hash::RandomXxHashBuilder;

pub fn new(samples: mpsc::UnboundedReceiver<Sample>,
           report: BiLock<Report>,
           max_batch_size: usize)
           -> Aggregator {
    Aggregator {
        samples: samples,
        report: report,
        max_batch_size: max_batch_size,
    }
}

/// Cooperatively aggregates `Sample`s into a `Report`.
///
/// This is mostly equivalent to calling `samples.fold()` with one notable caveat: if
/// `samples` _always_ contains metrics, it's possible for Fold to steal the event loop,
/// blocking other work. `Aggregator` limits the amount of work that
///
/// Furthermore, this allows us to avoid boxing a future.
pub struct Aggregator {
    samples: mpsc::UnboundedReceiver<Sample>,
    report: BiLock<Report>,
    max_batch_size: usize,
}

#[must_use = "futures do nothing unless polled"]
impl Future for Aggregator {
    type Item = ();
    type Error = ();
    fn poll(&mut self) -> Poll<(), ()> {
        trace!("poll");
        // First, obtain the report lock. It is unlocked when the report is dropped.
        match self.report.poll_lock() {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(mut report) => {
                // Then, obtain as many samples as possible, up to `max_batch_size`. The
                // number of samples obtained is limited so that the report lock isn't
                // held indefinitely (and, more generally, so that other things have a
                // chance to be scheduled).
                for _ in 0..self.max_batch_size {
                    match self.samples.poll()? {
                        Async::NotReady => return Ok(Async::NotReady),
                        Async::Ready(None) => return Ok(Async::Ready(())),
                        Async::Ready(Some(s)) => {
                            trace!("procesing sample: counters={}, gauges={}, stats={}",
                                   s.counters.len(),
                                   s.gauges.len(),
                                   s.stats.len());
                            // Update the report with new samples.
                            for (k, v) in &s.counters {
                                report.incr(k, *v);
                            }
                            for (k, v) in &s.gauges {
                                report.set(k, *v);
                            }
                            for (k, vs) in &s.stats {
                                report.add(k, vs);
                            }
                        }
                    }
                }

                // Yield back to the reactor so that work may continue, but inform the
                // reactor that the Aggregator is ready to continue working.
                trace!("yielding");
                task::park().unpark();
                Ok(Async::NotReady)
            }
        }
    }
}


/// Stores aggregated metrics.
#[derive(Clone)]
pub struct Report {
    pub counters: HashMap<CounterKey, u64, RandomXxHashBuilder>,
    pub gauges: HashMap<GaugeKey, u64, RandomXxHashBuilder>,
    pub stats: HashMap<StatKey, Histogram<u64>, RandomXxHashBuilder>,
}
impl Default for Report {
    fn default() -> Report {
        Report {
            counters: HashMap::default(),
            gauges: HashMap::default(),
            stats: HashMap::default(),
        }
    }
}

impl Report {
    pub fn new() -> Report {
        Default::default()
    }

    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.stats.is_empty()
    }

    pub fn len(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.stats.len()
    }

    fn incr(&mut self, k: &CounterKey, v: u64) {
        if let Some(mut curr) = self.counters.get_mut(k) {
            *curr += v;
            return;
        }
        self.counters.insert(k.clone(), v);
    }

    fn set(&mut self, k: &GaugeKey, v: u64) {
        if let Some(mut curr) = self.gauges.get_mut(k) {
            *curr = v;
            return;
        }
        self.gauges.insert(k.clone(), v);
    }

    fn add(&mut self, k: &StatKey, vs: &VecDeque<u64>) {
        if let Some(mut histo) = self.stats.get_mut(k) {
            for v in vs {
                if let Err(e) = histo.record(*v) {
                    error!("failed to add value to histogram: {:?}", e);
                }
            }
            return;
        }

        let low = k.low();
        let high = k.high();
        let mut histo = Histogram::<u64>::new_with_bounds(low, high, 4)
            .expect("failed to build Histogram");
        histo.auto(true);
        for v in vs {
            if let Err(e) = histo.record(*v) {
                error!("failed to add value to histogram: {:?}", e);
            }
        }
        self.stats.insert(k.clone(), histo);
    }

    pub fn reset(&mut self) {
        self.gauges.clear();
        self.stats.clear();
    }
}
