
use metrics::Metrics;

// Prints a report to stdout.
pub fn print_report(metrics: Metrics) {
    for (name, count) in metrics.counter_store.iter() {
        println!("{} {}", name, count);
    }

    for (name, histogram) in metrics.timer_store.iter() {
        println!("{}{{stat=\"count\"}} {}", name, histogram.count());
        // TODO: add sum()?
        //println!("{}{{stat=\"sum\"}} {}", name, histogram.sum());
        println!("{}{{stat=\"mean\"}} {}", name, histogram.mean());
        println!("{}{{stat=\"min\"}} {}", name, histogram.min());
        println!("{}{{stat=\"max\"}} {}", name, histogram.max());
        println!("{}{{stat=\"stddev\"}} {}", name, histogram.stdev());
        println!("{}{{stat=\"p50\"}} {}", name, histogram.value_at_percentile(0.5));
        println!("{}{{stat=\"p90\"}} {}", name, histogram.value_at_percentile(0.9));
        println!("{}{{stat=\"p95\"}} {}", name, histogram.value_at_percentile(0.95));
        println!("{}{{stat=\"p99\"}} {}", name, histogram.value_at_percentile(0.99));
        println!("{}{{stat=\"p9990\"}} {}", name, histogram.value_at_percentile(0.999));
        println!("{}{{stat=\"p9999\"}} {}", name, histogram.value_at_percentile(0.9999));
    }
}
