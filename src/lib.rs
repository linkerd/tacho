extern crate chrono;
extern crate futures;
extern crate hdrsample;
extern crate tokio_core;
extern crate tokio_timer;
#[macro_use]
extern crate log;
extern crate twox_hash;

use std::collections::VecDeque;

pub mod metrics;
pub mod reporter;
pub mod timer;

pub use metrics::{Metrics, Recorder};
pub use timer::Timer as Timed;

#[derive(Clone, Debug)]
pub struct Counter {
    pub name: String,
    pub value: u64,
}

impl Counter {
    pub fn new(name: String, init: u64) -> Counter {
        Counter {
            name: name,
            value: init,
        }
    }

    pub fn incr(&mut self, n: u64) {
        self.value += n;
    }

    pub fn fresh(&self) -> Counter {
        Counter {
            name: self.name.clone(),
            value: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Gauge {
    pub name: String,
    pub value: u64,
}

impl Gauge {
    pub fn new(name: String, n: u64) -> Gauge {
        Gauge {
            name: name,
            value: n,
        }
    }

    pub fn set(&mut self, n: u64) {
        self.value = n;
    }
}

pub struct Stat {
    pub name: String,
    pub values: VecDeque<u64>,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_incr() {
        let mut counter = Counter::new("foo".to_owned(), 3);
        counter.incr(1);
        counter.incr(1);
        counter.incr(1);
        assert!(counter.value == 6);
    }

    #[test]
    fn test_basic_gauges() {
        let mut v = Gauge::new("foo".into(), 123);
        v.set(432);
        assert_eq!(v.value, 432);
    }
}
