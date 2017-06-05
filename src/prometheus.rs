use super::Report;
use hdrsample::Histogram;
use std::fmt::{self, Display, Write};

pub fn string(report: &Report) -> Result<String, fmt::Error> {
    let mut out = String::with_capacity(16 * 1024);
    format(&mut out, report)?;
    Ok(out)
}

/// Renders a `Report` for Prometheus.
pub fn format<W: Write>(out: &mut W, report: &Report) -> fmt::Result {
    for (k, v) in report.counters() {
        let labels = FmtLabels::new(k.labels());
        if labels.is_empty() {
            writeln!(out, "{} {}", k.name(), v)?;
        } else {
            writeln!(out, "{}{{{}}} {}", k.name(), labels, v)?;
        }
    }

    for (k, v) in report.gauges() {
        let labels = FmtLabels::new(k.labels());
        if labels.is_empty() {
            writeln!(out, "{} {}", k.name(), v)?;
        } else {
            writeln!(out, "{}{{{}}} {}", k.name(), labels, v)?;
        }
    }

    for (k, h) in report.stats() {
        let name = k.name();
        let labels = FmtLabels::new(k.labels());

        format_stat(out,
                    &format_args!("{}_{}", name, "count"),
                    &labels,
                    &h.count())?;
        format_stat(out, &format_args!("{}_{}", name, "min"), &labels, &h.min())?;
        format_stat(out, &format_args!("{}_{}", name, "max"), &labels, &h.max())?;

        /// XXX for the time being, we export both quantiles and buckets so that we can come up
        format_quantiles(out, &name, &labels, h.histogram())?;
    }

    Ok(())
}

fn format_quantiles<N: fmt::Display, W: Write>(out: &mut W,
                                               name: &N,
                                               labels: &FmtLabels,
                                               h: &Histogram<usize>)
                                               -> fmt::Result {
    format_quantile(out, 0.5, name, labels, h)?;
    format_quantile(out, 0.9, name, labels, h)?;
    format_quantile(out, 0.95, name, labels, h)?;
    format_quantile(out, 0.99, name, labels, h)?;
    format_quantile(out, 0.999, name, labels, h)?;
    format_quantile(out, 0.9999, name, labels, h)?;
    Ok(())
}

fn format_quantile<N: fmt::Display, W: Write>(out: &mut W,
                                              quantile: f64,
                                              name: &N,
                                              labels: &FmtLabels,
                                              h: &Histogram<usize>)
                                              -> fmt::Result {
    let labels = labels.with_extra("quantile", format!("{}", quantile));
    format_stat(out, name, &labels, &h.value_at_percentile(quantile * 100.0))
}

fn format_stat<N: Display, V: Display, W: Write>(out: &mut W,
                                                 name: &N,
                                                 labels: &FmtLabels,
                                                 v: &V)
                                                 -> fmt::Result {
    writeln!(out, "{}{{{}}} {}", name, labels, v)
}

struct FmtLabels<'a> {
    base: &'a super::Labels,
    extra: super::Labels,
}
impl<'a> FmtLabels<'a> {
    fn new(base: &'a super::Labels) -> Self {
        FmtLabels {
            base,
            extra: super::Labels::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.base.is_empty() && self.extra.is_empty()
    }

    fn with_extra(&'a self, k: &'static str, v: String) -> FmtLabels<'a> {
        let mut extra = self.extra.clone();
        extra.insert(k, v);
        FmtLabels {
            base: self.base,
            extra,
        }
    }
}
impl<'a> fmt::Display for FmtLabels<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut written = 0;

        for (k, v) in self.extra.iter().chain(self.base.iter()) {
            if written == 0 {
                write!(f, "{}=\"{}\"", k, v)?;
                written += 1;
            } else {
                write!(f, ", {}=\"{}\"", k, v)?;
                written += 1;
            }
        }

        Ok(())
    }
}
