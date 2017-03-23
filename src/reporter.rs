use metrics::Metrics;
use chrono::offset::utc::UTC;
use std::io::{self, Write};

pub fn build_report(metrics: &Metrics) -> String {
    let mut report = String::with_capacity(8 * 1024);

    // TODO: time since last report.
    report.push_str(format!("metrics_generated_time_sec {}\n", UTC::now().timestamp()).as_str());

    report.push_str(format!("metrics{{count}} {}\n",
                            metrics.timer_store.len() + metrics.counter_store.len())
        .as_str());

    for (name, count) in &metrics.counter_store {
        report.push_str(format!("{} {}\n", name, count).as_str());
    }

    for (name, gauge) in &metrics.gauge_store {
        report.push_str(format!("{} {}\n", name, gauge).as_str());
    }

    for (name, histogram) in &metrics.timer_store {
        report.push_str(format!("{}{{stat=\"count\"}} {}\n", name, histogram.count()).as_str());
        report.push_str(format!("{}{{stat=\"mean\"}} {}\n", name, histogram.mean()).as_str());
        report.push_str(format!("{}{{stat=\"min\"}} {}\n", name, histogram.min()).as_str());
        report.push_str(format!("{}{{stat=\"max\"}} {}\n", name, histogram.max()).as_str());
        report.push_str(format!("{}{{stat=\"stddev\"}} {}\n", name, histogram.stdev()).as_str());
        report.push_str(format!("{}{{stat=\"p50\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.5))
            .as_str());
        report.push_str(format!("{}{{stat=\"p90\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.9))
            .as_str());
        report.push_str(format!("{}{{stat=\"p95\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.95))
            .as_str());
        report.push_str(format!("{}{{stat=\"p99\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.99))
            .as_str());
        report.push_str(format!("{}{{stat=\"p9990\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.999))
            .as_str());
        report.push_str(format!("{}{{stat=\"p9999\"}} {}\n",
                                name,
                                histogram.value_at_percentile(0.9999))
            .as_str());
    }

    report
}

// Prints a report to stdout.
pub fn print_report(metrics: &Metrics) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(build_report(metrics).as_bytes());
}
