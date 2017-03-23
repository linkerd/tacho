extern crate env_logger;

extern crate tachograph;
use tachograph::metrics::Metrics;
use tachograph::reporter;

// A performance test for a synchronous Metrics reporter with timers and counts.
//
// Creates 10 million counts, timing each loop and the cost of .incr().
fn main() {
    drop(env_logger::init());

    let mut metrics = Metrics::new();
    let mut loop_counter = metrics.make_counter("total_loops".to_owned());
    let mut loop_timer = metrics.make_timer("loop_timer_us".to_owned());
    let mut total_timer = metrics.make_timer("total_time_us".to_owned());

    let mut loops = 0;
    total_timer.start();
    while loops < 10_000_000 {
        loop_timer.start();
        {
            loop_counter.incr(1);
            // Do some work here.
        }
        loop_timer.stop();
        loop_timer = metrics.report_timer(loop_timer);
        loop_counter = metrics.report_counter(loop_counter);
        loops += 1;
    }
    total_timer.stop();
    metrics.report_timer(total_timer);

    reporter::print_report(&metrics);
}
