extern crate futures;
extern crate hdrsample;

use std::collections::HashMap;
use futures::{Stream, Sink, Future};
use timer::Timer;
use counter::Counter;
use hdrsample::Histogram;

pub struct Metrics {
    // TODO: swap these out with an xxHash HashMap for speed.
    // The default HashMap is built around a cryptographic hash.
    pub counter_store: HashMap<String, u64>,
    pub timer_store: HashMap<String, Histogram<u64>>,
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics {
            counter_store: HashMap::new(),
            timer_store: HashMap::new(),
        }
    }

    // Returns a Counter associated with this metrics object.
    pub fn make_counter(&mut self, name: String) -> Counter {
        let counter = Counter::new(name.clone());
        if !self.counter_store.contains_key(&name) {
          self.counter_store.insert(name, 0);
        }
        return counter;
    }

    pub fn make_timer(&mut self, name: String) -> Timer {
        let timer = Timer::new(name.clone());
        if !self.timer_store.contains_key(&name) {
            // TODO: this is one minute in microseconds.
            let histogram = Histogram::<u64>::new_with_bounds(1, 60 * 1000 * 1000, 5).unwrap();
            self.timer_store.insert(name, histogram);
        }
        return timer;
    }

    pub fn report_counter(&mut self, counter: Counter) -> Counter {
        if let Some(original_count) = self.counter_store.get_mut(&counter.name) {
            *original_count += counter.value;
        }
        return counter.fresh();
    }

    pub fn report_timer(&mut self, timer: Timer) -> Timer {
        if let Some(histogram) = self.timer_store.get_mut(&timer.name) {
            if let Some(elapsed) = timer.elapsed {
              histogram.record(elapsed);
            }
        }
        return timer.fresh();
    }

}

// Possible design of an aggregator:
// A Channel is fed with typed id, value (counts, gauges, timings)
// - Counts are added
// - Gauges are stored as-is with last-write-wins semantics (?)
// - Timings are aggregated in a hdr histogram per ID.
// How to do this:
// Have a future that does all the work, and use a select with a timeout
// that has the time resolution you're tracking. (like 1 minute or what have you)
// Benchmark different ways to aggregate:
// - combined channel
// - channel per type
//   - read until empty channel or read in a batch and give up time to other readers.
// Get a basic, fast-enough version in place and then write up benchmarking plans
// for after the tech preview. A channel per type should optimize fast enough and
// not add too much overhead.

#[cfg(test)]
mod tests {
    use super::Metrics;
    #[test]
    fn test_basic_metrics_1() {
        let metrics = Metrics::new();
    }
}