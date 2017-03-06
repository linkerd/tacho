#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;

extern crate chrono;
use chrono::prelude::*;

extern crate hdrsample;

pub mod counter;
pub mod timer;
pub mod metrics;
pub mod reporter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert!(true)
    }
}
