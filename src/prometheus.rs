use super::Report;
use hdrsample::Histogram;
use std::fmt;
use std::sync::Arc;

pub fn string(report: &Report) -> Result<String, fmt::Error> {
    let mut out = String::with_capacity(8 * 1024);
    write(&mut out, report)?;
    Ok(out)
}

/// Renders a `Report` for Prometheus.
pub fn write<W>(out: &mut W, report: &Report) -> fmt::Result
    where W: fmt::Write
{
    for (k, v) in report.counters() {
        let name = FmtName::new(k.prefix(), k.name());
        write_metric(out, &name, &k.labels().into(), v)?;
    }

    for (k, v) in report.gauges() {
        let name = FmtName::new(k.prefix(), k.name());
        write_metric(out, &name, &k.labels().into(), v)?;
    }

    for (k, h) in report.stats() {
        let name = FmtName::new(k.prefix(), k.name());
        let labels = k.labels().into();
        write_buckets(out, &name, &labels, h.histogram())?;
        write_metric(out, &format_args!("{}_{}", name, "min"), &labels, &h.min())?;
        write_metric(out, &format_args!("{}_{}", name, "max"), &labels, &h.max())?;
        write_metric(out, &format_args!("{}_{}", name, "sum"), &labels, &h.sum())?;
        write_metric(out,
                     &format_args!("{}_{}", name, "count"),
                     &labels,
                     &h.count())?;
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
    // `Histogram` tracks buckets as a sequence of minimum values and incremental counts,
    // however prometheus expects maximum values with cumulative counts.
    //
    // XXX Currently, we use the highest-granularity histogram available. This probably
    // isn't practical.
    let mut accum = 0;
    let mut count = 0;
    for bucket in h.iter_recorded() {
        if count > 0 {
            write_bucket(out, name, labels, &(bucket.value() - 1), accum)?;
        }
        count = bucket.count_at_value();
        accum += count;
    }
    if count > 0 {
        // Be explicit about the last bucket.
        write_bucket(out, name, labels, &h.max(), accum)?;
    }
    if accum > 0 {
        // Required to tell prom that the total count.
        write_bucket(out, name, labels, &"+Inf", accum)?;
    }
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
    write_metric(out,
                 &format_args!("{}_bucket", name),
                 &labels.with_extra("le", format_args!("{}", le)),
                 &count)
}

fn write_metric<W, N, V>(out: &mut W, name: &N, labels: &FmtLabels, v: &V) -> fmt::Result
    where W: fmt::Write,
          N: fmt::Display,
          V: fmt::Display
{
    writeln!(out, "{}{} {}", name, labels, v)
}

fn write_prefix<W>(out: &mut W, prefix: Arc<super::Prefix>) -> fmt::Result
    where W: fmt::Write
{
    if let super::Prefix::Node { ref prefix, value } = *prefix {
        write_prefix(out, prefix.clone())?;
        write!(out, "{}:", value)?;
    }
    Ok(())
}

/// Formats a prefixed name.
struct FmtName<'a> {
    prefix: &'a Arc<super::Prefix>,
    name: &'a str,
}

impl<'a> FmtName<'a> {
    fn new(prefix: &'a Arc<super::Prefix>, name: &'a str) -> Self {
        FmtName { prefix, name }
    }
}

impl<'a> fmt::Display for FmtName<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write_prefix(f, self.prefix.clone())?;
        write!(f, "{}", self.name)?;
        Ok(())
    }
}

impl<'a> From<&'a super::Labels> for FmtLabels<'a> {
    fn from(base: &'a super::Labels) -> Self {
        FmtLabels { base, extra: None }
    }
}

/// Formats labels.
struct FmtLabels<'a> {
    /// Labels from the original Key.
    base: &'a super::Labels,
    /// An export-specific label (for buckets, etc).
    extra: Option<(&'static str, fmt::Arguments<'a>)>,
}

impl<'a> FmtLabels<'a> {
    fn is_empty(&self) -> bool {
        self.base.is_empty() && self.extra.is_none()
    }

    /// Creates a new FmtLabels sharing a common `base` with a new copy of `extra`.
    fn with_extra(&'a self, k: &'static str, v: fmt::Arguments<'a>) -> FmtLabels<'a> {
        FmtLabels {
            base: self.base,
            extra: Some((k, v)),
        }
    }
}

impl<'a> fmt::Display for FmtLabels<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_empty() {
            return Ok(());
        }

        let mut first = true;
        write!(f, "{{")?;
        if let Some((k, v)) = self.extra {
            write!(f, "{}=\"{}\"", k, v)?;
            first = false;
        }
        for (k, v) in self.base.iter() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{}=\"{}\"", k, v)?;
            first = false;
        }
        write!(f, "}}")?;

        Ok(())
    }
}
