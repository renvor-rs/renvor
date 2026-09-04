//! A metrics port whose **cardinality is bounded at the port**.
//!
//! # Why the kernel has its own metrics port
//!
//! The ecosystem facades are general by design: `metrics` lets any call site attach any label
//! value, and `prometheus-client`'s `Family` creates a series for every label set it sees. Neither
//! can bound the number of series, and an unbounded series count is memory an attacker who
//! controls a label value — a queue name, a job kind, a route — grows for free. FR-071 asks for
//! the bound to be a property of the port, so it is:
//!
//! - label **names** are declared when an instrument is registered and cannot change;
//! - label **values** are bounded in length;
//! - the number of distinct label-value combinations per instrument is **capped**, and a
//!   combination beyond the cap is recorded against one `overflow` series rather than creating a
//!   new one.
//!
//! The port lives in the kernel so that the retry helper and every capability can count without
//! pulling a subscriber stack into their graphs; it has **no dependency**. Rendering — the
//! Prometheus text form — lives in `renvor-observability`, over [`Snapshot`].
//!
//! # Deterministic by construction
//!
//! [`Registry::snapshot`] returns families and series in a fixed order (by name, then by label
//! values), so a test asserts the exact document and a renderer is a pure function of it.

use core::fmt;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// The most bytes a label value may carry.
pub const MAX_LABEL_VALUE_BYTES: usize = 64;

/// The default cap on distinct label-value combinations per instrument.
pub const DEFAULT_MAX_SERIES: usize = 1024;

/// The most label names one instrument may declare.
pub const MAX_LABELS: usize = 8;

/// The value every label takes on the overflow series.
pub const OVERFLOW_VALUE: &str = "overflow";

/// Why an instrument could not be registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum MetricsError {
    /// The name did not match `[a-zA-Z_:][a-zA-Z0-9_:]*` or was empty or over 128 bytes.
    #[error("the instrument name is not a valid metric name")]
    InvalidName,
    /// A label name did not match `[a-zA-Z_][a-zA-Z0-9_]*`, was reserved, or repeated.
    #[error("a label name is invalid, reserved, or repeated")]
    InvalidLabelName,
    /// More than [`MAX_LABELS`] label names.
    #[error("too many label names")]
    TooManyLabels,
    /// The name was already registered with a different shape.
    #[error("the instrument name is already registered with a different shape")]
    AlreadyRegistered,
    /// A histogram was registered with no buckets or with unsorted buckets.
    #[error("histogram buckets must be non-empty and strictly increasing")]
    InvalidBuckets,
}

/// What kind of instrument a family is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InstrumentKind {
    /// Monotonically increasing.
    Counter,
    /// A value that goes up and down.
    Gauge,
    /// Observations bucketed by fixed bounds.
    Histogram,
}

impl InstrumentKind {
    /// The Prometheus type word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        }
    }
}

/// The shape an instrument was registered with, so a second registration must match it.
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    kind: InstrumentKind,
    help: &'static str,
    labels: Vec<&'static str>,
    buckets: Vec<f64>,
}

/// One series' storage.
#[derive(Debug)]
enum Storage {
    /// Fixed-point: counters and gauges hold their value as an integer number of thousandths so
    /// atomics can be used without a float CAS loop. Rendered as a float.
    Scalar(AtomicU64),
    /// Per-bucket counts (cumulative on render), the sum in thousandths, and the count.
    Histogram {
        buckets: Vec<AtomicU64>,
        sum_milli: AtomicU64,
        count: AtomicU64,
    },
}

/// One family: an instrument name and its series.
#[derive(Debug)]
struct Family {
    shape: Shape,
    max_series: usize,
    series: Mutex<BTreeMap<Vec<String>, Arc<Storage>>>,
    /// Combinations refused and folded into the overflow series.
    overflowed: AtomicU64,
}

/// The registry every instrument is created through. Cheap to clone; every clone shares one set.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    families: Mutex<BTreeMap<&'static str, Arc<Family>>>,
    max_series: Mutex<Option<usize>>,
}

impl Registry {
    /// Creates an empty registry with the default series cap.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry whose instruments cap at `max_series` distinct combinations.
    #[must_use]
    pub fn with_max_series(max_series: usize) -> Self {
        let registry = Self::default();
        *registry
            .inner
            .max_series
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(max_series.max(1));
        registry
    }

    fn max_series(&self) -> usize {
        self.inner
            .max_series
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(DEFAULT_MAX_SERIES)
    }

    /// Registers, or returns the existing, counter `name` with the given label names.
    ///
    /// # Errors
    ///
    /// A [`MetricsError`] for an invalid name or label set, or a name already registered with a
    /// different shape.
    pub fn counter(
        &self,
        name: &'static str,
        help: &'static str,
        labels: &[&'static str],
    ) -> Result<Counter, MetricsError> {
        let family = self.family(name, help, labels, InstrumentKind::Counter, Vec::new())?;
        Ok(Counter { family })
    }

    /// Registers, or returns the existing, gauge `name`.
    ///
    /// # Errors
    ///
    /// As [`Self::counter`].
    pub fn gauge(
        &self,
        name: &'static str,
        help: &'static str,
        labels: &[&'static str],
    ) -> Result<Gauge, MetricsError> {
        let family = self.family(name, help, labels, InstrumentKind::Gauge, Vec::new())?;
        Ok(Gauge { family })
    }

    /// Registers, or returns the existing, histogram `name` with fixed `buckets` (upper bounds,
    /// strictly increasing; `+Inf` is implicit).
    ///
    /// # Errors
    ///
    /// As [`Self::counter`], plus [`MetricsError::InvalidBuckets`].
    pub fn histogram(
        &self,
        name: &'static str,
        help: &'static str,
        labels: &[&'static str],
        buckets: &[f64],
    ) -> Result<Histogram, MetricsError> {
        if buckets.is_empty()
            || buckets.iter().any(|bound| !bound.is_finite())
            || buckets.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(MetricsError::InvalidBuckets);
        }
        let family = self.family(
            name,
            help,
            labels,
            InstrumentKind::Histogram,
            buckets.to_vec(),
        )?;
        Ok(Histogram { family })
    }

    fn family(
        &self,
        name: &'static str,
        help: &'static str,
        labels: &[&'static str],
        kind: InstrumentKind,
        buckets: Vec<f64>,
    ) -> Result<Arc<Family>, MetricsError> {
        if !valid_metric_name(name) {
            return Err(MetricsError::InvalidName);
        }
        if labels.len() > MAX_LABELS {
            return Err(MetricsError::TooManyLabels);
        }
        for (index, label) in labels.iter().enumerate() {
            if !valid_label_name(label) || labels[..index].contains(label) {
                return Err(MetricsError::InvalidLabelName);
            }
        }
        let shape = Shape {
            kind,
            help,
            labels: labels.to_vec(),
            buckets,
        };
        let mut families = self
            .inner
            .families
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = families.get(name) {
            return if existing.shape == shape {
                Ok(Arc::clone(existing))
            } else {
                Err(MetricsError::AlreadyRegistered)
            };
        }
        let family = Arc::new(Family {
            shape,
            max_series: self.max_series(),
            series: Mutex::new(BTreeMap::new()),
            overflowed: AtomicU64::new(0),
        });
        families.insert(name, Arc::clone(&family));
        Ok(family)
    }

    /// Every family and series, in a fixed order.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let families = self
            .inner
            .families
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::with_capacity(families.len());
        for (name, family) in families.iter() {
            let series = family.series.lock().unwrap_or_else(PoisonError::into_inner);
            let mut rows = Vec::with_capacity(series.len());
            for (values, storage) in series.iter() {
                let labels: Vec<(&'static str, String)> = family
                    .shape
                    .labels
                    .iter()
                    .copied()
                    .zip(values.iter().cloned())
                    .collect();
                let value = match &**storage {
                    Storage::Scalar(cell) => {
                        SeriesValue::Scalar(from_milli(cell.load(Ordering::Relaxed)))
                    }
                    Storage::Histogram {
                        buckets,
                        sum_milli,
                        count,
                    } => {
                        let mut cumulative = 0_u64;
                        let counts = buckets
                            .iter()
                            .map(|cell| {
                                cumulative += cell.load(Ordering::Relaxed);
                                cumulative
                            })
                            .collect();
                        SeriesValue::Histogram {
                            bounds: family.shape.buckets.clone(),
                            cumulative_counts: counts,
                            sum: from_milli(sum_milli.load(Ordering::Relaxed)),
                            count: count.load(Ordering::Relaxed),
                        }
                    }
                };
                rows.push(Series { labels, value });
            }
            out.push(FamilySnapshot {
                name,
                help: family.shape.help,
                kind: family.shape.kind,
                series: rows,
                overflowed: family.overflowed.load(Ordering::Relaxed),
            });
        }
        Snapshot { families: out }
    }
}

impl Family {
    /// The storage for one label set, creating it if the cap allows and folding into the
    /// overflow series otherwise.
    fn storage(&self, labels: &[(&str, &str)]) -> Arc<Storage> {
        let key: Option<Vec<String>> = if labels.len() == self.shape.labels.len()
            && labels
                .iter()
                .zip(self.shape.labels.iter())
                .all(|((name, value), declared)| {
                    name == declared
                        && value.len() <= MAX_LABEL_VALUE_BYTES
                        && !value.contains(['\n', '\0'])
                }) {
            Some(
                labels
                    .iter()
                    .map(|(_, value)| (*value).to_owned())
                    .collect(),
            )
        } else {
            None
        };
        let mut series = self.series.lock().unwrap_or_else(PoisonError::into_inner);
        let overflow_key = || vec![OVERFLOW_VALUE.to_owned(); self.shape.labels.len()];
        let key = match key {
            Some(key) if series.contains_key(&key) => key,
            // A new combination is admitted only under the cap. The overflow series does not
            // count against it, so the true series bound is `max_series + 1`.
            Some(key)
                if series
                    .len()
                    .saturating_sub(usize::from(series.contains_key(&overflow_key())))
                    < self.max_series =>
            {
                key
            }
            _ => {
                self.overflowed.fetch_add(1, Ordering::Relaxed);
                overflow_key()
            }
        };
        Arc::clone(series.entry(key).or_insert_with(|| {
            Arc::new(match self.shape.kind {
                InstrumentKind::Counter | InstrumentKind::Gauge => {
                    Storage::Scalar(AtomicU64::new(0))
                }
                InstrumentKind::Histogram => Storage::Histogram {
                    buckets: (0..=self.shape.buckets.len())
                        .map(|_| AtomicU64::new(0))
                        .collect(),
                    sum_milli: AtomicU64::new(0),
                    count: AtomicU64::new(0),
                },
            })
        }))
    }
}

/// A registered counter.
#[derive(Clone, Debug)]
pub struct Counter {
    family: Arc<Family>,
}

impl Counter {
    /// Adds `by` to the series for `labels`.
    ///
    /// `labels` must name exactly the registered label names, in order; anything else — a wrong
    /// name, a missing label, an over-long value, a value with a newline — is folded into the
    /// overflow series rather than refused, because a counter that can fail is a counter nobody
    /// checks.
    pub fn increment(&self, labels: &[(&str, &str)], by: u64) {
        if let Storage::Scalar(cell) = &*self.family.storage(labels) {
            cell.fetch_add(by.saturating_mul(1_000), Ordering::Relaxed);
        }
    }
}

/// A registered gauge.
#[derive(Clone, Debug)]
pub struct Gauge {
    family: Arc<Family>,
}

impl Gauge {
    /// Sets the series for `labels` to `value`. Negative and non-finite values clamp to zero —
    /// a gauge here is a count of things, and the atomic storage is unsigned.
    pub fn set(&self, labels: &[(&str, &str)], value: f64) {
        if let Storage::Scalar(cell) = &*self.family.storage(labels) {
            cell.store(to_milli(value), Ordering::Relaxed);
        }
    }
}

/// A registered histogram.
#[derive(Clone, Debug)]
pub struct Histogram {
    family: Arc<Family>,
}

impl Histogram {
    /// Records one observation.
    pub fn observe(&self, labels: &[(&str, &str)], value: f64) {
        if let Storage::Histogram {
            buckets,
            sum_milli,
            count,
        } = &*self.family.storage(labels)
        {
            let index = self
                .family
                .shape
                .buckets
                .iter()
                .position(|bound| value <= *bound)
                .unwrap_or(self.family.shape.buckets.len());
            buckets[index].fetch_add(1, Ordering::Relaxed);
            sum_milli.fetch_add(to_milli(value), Ordering::Relaxed);
            count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A point-in-time reading of every family.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Families, ordered by name.
    pub families: Vec<FamilySnapshot>,
}

/// One family's reading.
#[derive(Clone, Debug, PartialEq)]
pub struct FamilySnapshot {
    /// The instrument name.
    pub name: &'static str,
    /// The help text.
    pub help: &'static str,
    /// The kind.
    pub kind: InstrumentKind,
    /// Series, ordered by label values.
    pub series: Vec<Series>,
    /// How many combinations were folded into the overflow series.
    pub overflowed: u64,
}

/// One series' reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Label name and value pairs in declaration order.
    pub labels: Vec<(&'static str, String)>,
    /// The value.
    pub value: SeriesValue,
}

/// A series' value.
#[derive(Clone, Debug, PartialEq)]
pub enum SeriesValue {
    /// A counter or gauge.
    Scalar(f64),
    /// A histogram: cumulative counts per bound (the implicit `+Inf` is the last element), the
    /// sum, and the count.
    Histogram {
        /// The upper bounds, as registered.
        bounds: Vec<f64>,
        /// Cumulative counts, one per bound plus one for `+Inf`.
        cumulative_counts: Vec<u64>,
        /// The sum of observations.
        sum: f64,
        /// The number of observations.
        count: u64,
    },
}

impl fmt::Display for InstrumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `[a-zA-Z_:][a-zA-Z0-9_:]*`, 1–128 bytes.
fn valid_metric_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && (bytes[0].is_ascii_alphabetic() || matches!(bytes[0], b'_' | b':'))
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
}

/// `[a-zA-Z_][a-zA-Z0-9_]*`, 1–64 bytes, not starting with `__` (reserved by Prometheus).
fn valid_label_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && !name.starts_with("__")
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn to_milli(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    // Saturate rather than wrap: a gauge of 2^64 thousandths is not a value anyone reads.
    (value * 1_000.0).min(u64::MAX as f64) as u64
}

fn from_milli(milli: u64) -> f64 {
    milli as f64 / 1_000.0
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_SERIES, InstrumentKind, MAX_LABEL_VALUE_BYTES, MetricsError, OVERFLOW_VALUE,
        Registry, SeriesValue,
    };

    #[test]
    fn names_and_labels_are_validated_at_registration() {
        let registry = Registry::new();
        assert_eq!(
            registry.counter("9bad", "h", &[]).unwrap_err(),
            MetricsError::InvalidName
        );
        assert_eq!(
            registry
                .counter("ok_name", "h", &["__reserved"])
                .unwrap_err(),
            MetricsError::InvalidLabelName
        );
        assert_eq!(
            registry.counter("ok_name", "h", &["a", "a"]).unwrap_err(),
            MetricsError::InvalidLabelName
        );
        assert_eq!(
            registry
                .counter(
                    "ok_name",
                    "h",
                    &["a", "b", "c", "d", "e", "f", "g", "h", "i"]
                )
                .unwrap_err(),
            MetricsError::TooManyLabels
        );
        assert_eq!(
            registry.histogram("h", "h", &[], &[2.0, 1.0]).unwrap_err(),
            MetricsError::InvalidBuckets
        );
        // POSITIVE CONTROL, and the same shape twice is the same family.
        assert!(
            registry
                .counter("renvor_jobs_total", "jobs", &["queue"])
                .is_ok()
        );
        assert!(
            registry
                .counter("renvor_jobs_total", "jobs", &["queue"])
                .is_ok()
        );
        assert_eq!(
            registry
                .counter("renvor_jobs_total", "jobs", &["kind"])
                .unwrap_err(),
            MetricsError::AlreadyRegistered
        );
    }

    #[test]
    fn a_counter_counts_per_label_set_in_a_deterministic_order() {
        let registry = Registry::new();
        let counter = registry
            .counter("renvor_jobs_total", "jobs", &["queue", "outcome"])
            .unwrap();
        counter.increment(&[("queue", "mail"), ("outcome", "ok")], 2);
        counter.increment(&[("queue", "default"), ("outcome", "ok")], 1);
        counter.increment(&[("queue", "mail"), ("outcome", "ok")], 3);

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.families.len(), 1);
        let family = &snapshot.families[0];
        assert_eq!(family.kind, InstrumentKind::Counter);
        assert_eq!(family.series.len(), 2);
        assert_eq!(family.series[0].labels[0].1, "default");
        assert_eq!(family.series[0].value, SeriesValue::Scalar(1.0));
        assert_eq!(family.series[1].labels[0].1, "mail");
        assert_eq!(family.series[1].value, SeriesValue::Scalar(5.0));
    }

    #[test]
    fn combinations_beyond_the_cap_fold_into_one_overflow_series() {
        let registry = Registry::with_max_series(2);
        let counter = registry.counter("c", "h", &["k"]).unwrap();
        for value in ["a", "b", "c", "d"] {
            counter.increment(&[("k", value)], 1);
        }
        let family = &registry.snapshot().families[0];
        let values: Vec<&str> = family
            .series
            .iter()
            .map(|series| series.labels[0].1.as_str())
            .collect();
        assert_eq!(values, vec!["a", "b", OVERFLOW_VALUE]);
        assert_eq!(family.series[2].value, SeriesValue::Scalar(2.0));
        assert_eq!(family.overflowed, 2);
        // The default cap is the documented number.
        assert_eq!(DEFAULT_MAX_SERIES, 1024);
    }

    #[test]
    fn a_wrong_or_oversized_label_set_is_folded_rather_than_refused() {
        let registry = Registry::new();
        let counter = registry.counter("c", "h", &["k"]).unwrap();
        counter.increment(&[("wrong", "x")], 1);
        counter.increment(&[("k", &"x".repeat(MAX_LABEL_VALUE_BYTES + 1))], 1);
        counter.increment(&[("k", "with\nnewline")], 1);
        counter.increment(&[], 1);
        let family = &registry.snapshot().families[0];
        assert_eq!(family.series.len(), 1);
        assert_eq!(family.series[0].labels[0].1, OVERFLOW_VALUE);
        assert_eq!(family.series[0].value, SeriesValue::Scalar(4.0));
        // POSITIVE CONTROL: a value at the bound is admitted.
        counter.increment(&[("k", &"x".repeat(MAX_LABEL_VALUE_BYTES))], 1);
        assert_eq!(registry.snapshot().families[0].series.len(), 2);
    }

    #[test]
    fn histograms_bucket_cumulatively_with_an_implicit_infinity() {
        let registry = Registry::new();
        let histogram = registry
            .histogram("renvor_job_seconds", "h", &[], &[0.1, 1.0, 10.0])
            .unwrap();
        for value in [0.05, 0.5, 5.0, 50.0] {
            histogram.observe(&[], value);
        }
        let family = &registry.snapshot().families[0];
        match &family.series[0].value {
            SeriesValue::Histogram {
                bounds,
                cumulative_counts,
                sum,
                count,
            } => {
                assert_eq!(bounds, &vec![0.1, 1.0, 10.0]);
                assert_eq!(cumulative_counts, &vec![1, 2, 3, 4]);
                assert!((sum - 55.55).abs() < 1e-9, "sum {sum}");
                assert_eq!(*count, 4);
            }
            other => panic!("not a histogram: {other:?}"),
        }
    }

    #[test]
    fn gauges_set_and_clamp() {
        let registry = Registry::new();
        let gauge = registry.gauge("g", "h", &[]).unwrap();
        gauge.set(&[], 3.5);
        assert_eq!(
            registry.snapshot().families[0].series[0].value,
            SeriesValue::Scalar(3.5)
        );
        gauge.set(&[], -1.0);
        assert_eq!(
            registry.snapshot().families[0].series[0].value,
            SeriesValue::Scalar(0.0)
        );
        gauge.set(&[], f64::NAN);
        assert_eq!(
            registry.snapshot().families[0].series[0].value,
            SeriesValue::Scalar(0.0)
        );
    }

    #[test]
    fn clones_share_one_registry() {
        let registry = Registry::new();
        let clone = registry.clone();
        clone.counter("c", "h", &[]).unwrap().increment(&[], 1);
        assert_eq!(registry.snapshot().families.len(), 1);
    }
}
