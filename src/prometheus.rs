use super::Report;
use hdrsample::Histogram;
use std::fmt;

pub fn string(report: &Report) -> Result<String, fmt::Error> {
    let mut out = String::with_capacity(16 * 1024);
    write(&mut out, report)?;
    Ok(out)
}

/// Renders a `Report` for Prometheus.
pub fn write<W>(out: &mut W, report: &Report) -> fmt::Result
    where W: fmt::Write
{
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

        write_stat(out,
                   &format_args!("{}_{}", name, "count"),
                   &labels,
                   &h.count())?;
        write_stat(out, &format_args!("{}_{}", name, "sum"), &labels, &h.sum())?;
        write_stat(out, &format_args!("{}_{}", name, "min"), &labels, &h.min())?;
        write_stat(out, &format_args!("{}_{}", name, "max"), &labels, &h.max())?;

        /// XXX for the time being, we export both quantiles and buckets so that we can
        /// compare.
        let histogram = h.histogram();
        write_quantiles(out, &name, &labels, histogram)?;
        write_buckets(out, &name, &labels, histogram)?;
    }

    Ok(())
}

fn write_buckets<N, W>(out: &mut W,
                       name: &N,
                       labels: &FmtLabels,
                       h: &Histogram<usize>)
                       -> fmt::Result
    where N: fmt::Display,
          W: fmt::Write
{
    let mut accum = 0;
    // XXX for now just dump all
    for bucket in h.iter_recorded() {
        if accum > 0 {
            write_bucket(out, name, labels, &(bucket.value() - 1), accum)?;
        }
        accum += bucket.count_at_value();
    }
    write_bucket(out, name, labels, &h.max(), accum)?; // Be explicit about the last bucket.
    write_bucket(out, name, labels, &"+Inf", accum)?; // Required to tell prom that this is the total.
    Ok(())
}

fn write_bucket<N, M, W>(out: &mut W,
                         name: &N,
                         labels: &FmtLabels,
                         le: &M,
                         count: usize)
                         -> fmt::Result
    where N: fmt::Display,
          M: fmt::Display,
          W: fmt::Write
{
    let labels = labels.with_extra("le", format!("{}", le));
    write_stat(out, &format_args!("{}_bucket", name), &labels, &count)
}

fn write_quantiles<N, W>(out: &mut W,
                         name: &N,
                         labels: &FmtLabels,
                         h: &Histogram<usize>)
                         -> fmt::Result
    where N: fmt::Display,
          W: fmt::Write
{
    write_quantile(out, 0.5, name, labels, h)?;
    write_quantile(out, 0.9, name, labels, h)?;
    write_quantile(out, 0.95, name, labels, h)?;
    write_quantile(out, 0.99, name, labels, h)?;
    write_quantile(out, 0.999, name, labels, h)?;
    write_quantile(out, 0.9999, name, labels, h)?;
    Ok(())
}

fn write_quantile<N, W>(out: &mut W,
                        quantile: f64,
                        name: &N,
                        labels: &FmtLabels,
                        h: &Histogram<usize>)
                        -> fmt::Result
    where N: fmt::Display,
          W: fmt::Write
{
    let labels = labels.with_extra("quantile", format!("{}", quantile));
    write_stat(out, name, &labels, &h.value_at_percentile(quantile * 100.0))
}

fn write_stat<W, N, V>(out: &mut W, name: &N, labels: &FmtLabels, v: &V) -> fmt::Result
    where W: fmt::Write,
          N: fmt::Display,
          V: fmt::Display
{
    writeln!(out, "{}{{{}}} {}", name, labels, v)
}

/// Supports formatting labels.
struct FmtLabels<'a> {
    /// Labels from the original Key.
    base: &'a super::Labels,
    /// Export-specific labels.
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

    /// Creates a new FmtLabels sharing a common `base` with a new copy of `extra`.
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
