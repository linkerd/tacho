use std::time::{Instant, Duration};

pub trait Timing {
    fn elapsed_us(&self) -> u64;
    fn elapsed_ms(&self) -> u64;
}

impl Timing {
    pub fn start() -> Instant {
        Instant::now()
    }
}

impl Timing for Duration {
    fn elapsed_us(&self) -> u64 {
        self.as_secs() as u64 * 1_000_000 + u64::from(self.subsec_nanos()) / 1_000
    }
    fn elapsed_ms(&self) -> u64 {
        self.as_secs() as u64 * 1_000 + u64::from(self.subsec_nanos()) / 1_000_000
    }
}

impl Timing for Instant {
    fn elapsed_us(&self) -> u64 {
        self.elapsed().elapsed_us()
    }
    fn elapsed_ms(&self) -> u64 {
        self.elapsed().elapsed_ms()
    }
}

#[test]
fn test_conversions() {
    let d = Duration::new(54, 321_987_600);
    assert_eq!(d.elapsed_us(), 54_321_987);
    assert_eq!(d.elapsed_ms(), 54_321);
}
