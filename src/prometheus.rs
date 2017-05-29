use super::Report;
use std::collections::BTreeMap;
use std::fmt::Display;

// The initial size
const BUF_SIZE: usize = 8 * 1024;

/// Renders a `Report` for Prometheus.
pub fn format<R: Report>(report: &R) -> String {
    let mut out = String::with_capacity(BUF_SIZE);

    for (k, v) in report.counters() {
        let labels = k.labels();
        if labels.is_empty() {
            out.push_str(&format!("{} {}\n", k.name(), v));
        } else {
            out.push_str(&format!("{}{{{}}} {}\n", k.name(), &format_labels(labels), v));
        }
    }

    for (k, v) in report.gauges() {
        let labels = k.labels();
        if labels.is_empty() {
            out.push_str(&format!("{} {}\n", k.name(), v));
        } else {
            out.push_str(&format!("{}{{{}}} {}\n", k.name(), &format_labels(labels), v));
        }
    }

    for (k, h) in report.stats() {
        let name = k.name();
        let labels = {
            let labels = k.labels();
            if labels.is_empty() {
                "".to_string()
            } else {
                format!(", {}", format_labels(labels))
            }
        };
        let labels = &labels;
        out.push_str(&format_stat("count", name, labels, h.count()));
        out.push_str(&format_stat("min", name, labels, h.min()));
        out.push_str(&format_stat("max", name, labels, h.max()));
        out.push_str(&format_stat("stddev", name, labels, h.stdev()));
        out.push_str(&format_stat("p50", name, labels, h.value_at_percentile(50.0)));
        out.push_str(&format_stat("p90", name, labels, h.value_at_percentile(90.0)));
        out.push_str(&format_stat("p95", name, labels, h.value_at_percentile(95.0)));
        out.push_str(&format_stat("p99", name, labels, h.value_at_percentile(99.0)));
        out.push_str(&format_stat("p999", name, labels, h.value_at_percentile(99.9)));
        out.push_str(&format_stat("p9999", name, labels, h.value_at_percentile(99.99)));
    }

    out
}

fn format_stat<V: Display>(stat: &str, name: &str, labels: &str, v: V) -> String {
    let out = format!("{}{{stat=\"{}\"{}}} {}\n", name, stat, labels, v);
    drop(v); // this is really just to appease clippy.
    out
}

fn format_labels(labels: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(16 * 1024);
    let sz = labels.len();
    for (i, (k, v)) in labels.iter().enumerate() {
        out.push_str(&format!("{}=\"{}\"", k, v));
        if i < sz - 1 {
            out.push_str(", ");
        }
    }
    out
}
