//! In-process recorder that makes the `metrics` facade useful by default.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};

/// A dynamically composed metrics recorder accepted from export adapters.
pub type BoxedMetricsRecorder = Arc<dyn Recorder + Send + Sync + 'static>;

/// Bounds applied to metric names and label cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsPolicy {
    /// Maximum number of unique counter, gauge, and histogram series.
    pub max_series: usize,
    /// Maximum number of labels on one metric series.
    pub max_labels_per_series: usize,
    /// Maximum UTF-8 byte length of a metric name, label name, or label value.
    pub max_field_length: usize,
}

impl Default for MetricsPolicy {
    fn default() -> Self {
        Self {
            max_series: 4_096,
            max_labels_per_series: 32,
            max_field_length: 256,
        }
    }
}

impl MetricsPolicy {
    /// Returns whether one metric identity is within the configured bounds.
    pub fn allows(&self, key: &Key) -> bool {
        metric_key(key, *self).is_some()
    }
}

/// Summary of recorded histogram values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistogramSnapshot {
    /// Number of recorded values.
    pub count: u64,
    /// Sum of all values.
    pub sum: f64,
    /// Minimum value, if any.
    pub min: Option<f64>,
    /// Maximum value, if any.
    pub max: Option<f64>,
}

/// Snapshot value of a metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricValue {
    /// Monotonic counter.
    Counter(u64),
    /// Last gauge value.
    Gauge(f64),
    /// Histogram summary.
    Histogram(HistogramSnapshot),
}

/// One metric and its stable key.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricSnapshot {
    /// Metric key including its labels.
    pub key: String,
    /// Current recorded value.
    pub value: MetricValue,
}

/// Default in-memory `metrics` recorder.
///
/// Export adapters may replace this recorder, but generated applications have
/// working counters, gauges, and histograms even without an external backend.
#[derive(Debug, Clone)]
pub struct InMemoryRecorder {
    counters: Arc<RwLock<BTreeMap<String, Arc<AtomicU64>>>>,
    gauges: Arc<RwLock<BTreeMap<String, Arc<AtomicU64>>>>,
    histograms: Arc<RwLock<BTreeMap<String, Arc<HistogramState>>>>,
    policy: MetricsPolicy,
    series: Arc<AtomicUsize>,
    dropped_series: Arc<AtomicU64>,
}

impl Default for InMemoryRecorder {
    fn default() -> Self {
        Self::new(MetricsPolicy::default())
    }
}

impl InMemoryRecorder {
    /// Creates a recorder with explicit cardinality bounds.
    pub fn new(policy: MetricsPolicy) -> Self {
        Self {
            counters: Arc::new(RwLock::new(BTreeMap::new())),
            gauges: Arc::new(RwLock::new(BTreeMap::new())),
            histograms: Arc::new(RwLock::new(BTreeMap::new())),
            policy,
            series: Arc::new(AtomicUsize::new(0)),
            dropped_series: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns a deterministic snapshot for health endpoints and tests.
    pub fn snapshot(&self) -> Vec<MetricSnapshot> {
        let mut snapshots = Vec::new();
        snapshots.extend(read_unpoisoned(&self.counters).iter().map(|(key, value)| {
            MetricSnapshot {
                key: key.clone(),
                value: MetricValue::Counter(value.load(Ordering::Acquire)),
            }
        }));
        snapshots.extend(
            read_unpoisoned(&self.gauges)
                .iter()
                .map(|(key, value)| MetricSnapshot {
                    key: key.clone(),
                    value: MetricValue::Gauge(f64::from_bits(value.load(Ordering::Acquire))),
                }),
        );
        snapshots.extend(
            read_unpoisoned(&self.histograms)
                .iter()
                .map(|(key, value)| MetricSnapshot {
                    key: key.clone(),
                    value: MetricValue::Histogram(value.snapshot()),
                }),
        );
        snapshots.sort_by(|left, right| left.key.cmp(&right.key));
        snapshots
    }

    /// Returns the number of registrations rejected by the cardinality policy.
    pub fn dropped_series(&self) -> u64 {
        self.dropped_series.load(Ordering::Acquire)
    }

    fn reserve_series(&self) -> bool {
        self.series
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.policy.max_series).then_some(current + 1)
            })
            .is_ok()
    }

    fn reject_series(&self) {
        self.dropped_series.fetch_add(1, Ordering::Relaxed);
    }
}

impl Recorder for InMemoryRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let Some(key) = metric_key(key, self.policy) else {
            self.reject_series();
            return Counter::noop();
        };
        let mut counters = write_unpoisoned(&self.counters);
        if let Some(value) = counters.get(&key) {
            return Counter::from_arc(value.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Counter::noop();
        }
        let value = Arc::new(AtomicU64::new(0));
        counters.insert(key, value.clone());
        Counter::from_arc(value)
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let Some(key) = metric_key(key, self.policy) else {
            self.reject_series();
            return Gauge::noop();
        };
        let mut gauges = write_unpoisoned(&self.gauges);
        if let Some(value) = gauges.get(&key) {
            return Gauge::from_arc(value.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Gauge::noop();
        }
        let value = Arc::new(AtomicU64::new(0.0_f64.to_bits()));
        gauges.insert(key, value.clone());
        Gauge::from_arc(value)
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        let Some(key) = metric_key(key, self.policy) else {
            self.reject_series();
            return Histogram::noop();
        };
        let mut histograms = write_unpoisoned(&self.histograms);
        if let Some(value) = histograms.get(&key) {
            return Histogram::from_arc(value.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Histogram::noop();
        }
        let value = Arc::new(HistogramState::default());
        histograms.insert(key, value.clone());
        Histogram::from_arc(value)
    }
}

pub(crate) struct FanoutRecorder {
    primary: BoxedMetricsRecorder,
    exporter: BoxedMetricsRecorder,
}

impl FanoutRecorder {
    pub(crate) fn new(primary: BoxedMetricsRecorder, exporter: BoxedMetricsRecorder) -> Self {
        Self { primary, exporter }
    }
}

impl Recorder for FanoutRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.primary
            .describe_counter(key.clone(), unit, description.clone());
        self.exporter.describe_counter(key, unit, description);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.primary
            .describe_gauge(key.clone(), unit, description.clone());
        self.exporter.describe_gauge(key, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.primary
            .describe_histogram(key.clone(), unit, description.clone());
        self.exporter.describe_histogram(key, unit, description);
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(FanoutCounter {
            primary: self.primary.register_counter(key, metadata),
            exporter: self.exporter.register_counter(key, metadata),
        }))
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        Gauge::from_arc(Arc::new(FanoutGauge {
            primary: self.primary.register_gauge(key, metadata),
            exporter: self.exporter.register_gauge(key, metadata),
        }))
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(FanoutHistogram {
            primary: self.primary.register_histogram(key, metadata),
            exporter: self.exporter.register_histogram(key, metadata),
        }))
    }
}

struct FanoutCounter {
    primary: Counter,
    exporter: Counter,
}

impl CounterFn for FanoutCounter {
    fn increment(&self, value: u64) {
        self.primary.increment(value);
        self.exporter.increment(value);
    }

    fn absolute(&self, value: u64) {
        self.primary.absolute(value);
        self.exporter.absolute(value);
    }
}

struct FanoutGauge {
    primary: Gauge,
    exporter: Gauge,
}

impl GaugeFn for FanoutGauge {
    fn increment(&self, value: f64) {
        self.primary.increment(value);
        self.exporter.increment(value);
    }

    fn decrement(&self, value: f64) {
        self.primary.decrement(value);
        self.exporter.decrement(value);
    }

    fn set(&self, value: f64) {
        self.primary.set(value);
        self.exporter.set(value);
    }
}

struct FanoutHistogram {
    primary: Histogram,
    exporter: Histogram,
}

impl HistogramFn for FanoutHistogram {
    fn record(&self, value: f64) {
        self.primary.record(value);
        self.exporter.record(value);
    }

    fn record_many(&self, value: f64, count: usize) {
        self.primary.record_many(value, count);
        self.exporter.record_many(value, count);
    }
}

#[derive(Debug, Default)]
struct HistogramState {
    inner: Mutex<HistogramSnapshot>,
}

impl Default for HistogramSnapshot {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
        }
    }
}

impl HistogramState {
    fn snapshot(&self) -> HistogramSnapshot {
        *lock_unpoisoned(&self.inner)
    }
}

impl HistogramFn for HistogramState {
    fn record(&self, value: f64) {
        let mut snapshot = lock_unpoisoned(&self.inner);
        snapshot.count = snapshot.count.saturating_add(1);
        snapshot.sum += value;
        snapshot.min = Some(snapshot.min.map_or(value, |current| current.min(value)));
        snapshot.max = Some(snapshot.max.map_or(value, |current| current.max(value)));
    }
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn lock_unpoisoned<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn metric_key(key: &Key, policy: MetricsPolicy) -> Option<String> {
    if key.name().is_empty() || key.name().len() > policy.max_field_length {
        return None;
    }
    let mut labels = key
        .labels()
        .map(|label| (label.key(), label.value()))
        .collect::<Vec<_>>();
    if labels.len() > policy.max_labels_per_series
        || labels.iter().any(|(name, value)| {
            name.is_empty()
                || name.len() > policy.max_field_length
                || value.len() > policy.max_field_length
        })
    {
        return None;
    }
    if labels.is_empty() {
        return Some(key.name().to_owned());
    }
    labels.sort_unstable();
    let labels = labels
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("{}{{{labels}}}", key.name()))
}

#[cfg(test)]
mod tests {
    use metrics::Recorder;

    use super::*;

    #[test]
    fn records_all_three_metric_kinds() {
        let recorder = InMemoryRecorder::default();
        let key = Key::from_name("ddd4r.requests");
        let metadata = Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        recorder.register_counter(&key, &metadata).increment(2);
        recorder.register_gauge(&key, &metadata).set(3.5);
        recorder.register_histogram(&key, &metadata).record(8.0);

        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.len(), 3);
        assert!(
            snapshot
                .iter()
                .any(|metric| metric.value == MetricValue::Counter(2))
        );
        assert!(
            snapshot
                .iter()
                .any(|metric| metric.value == MetricValue::Gauge(3.5))
        );
    }

    #[test]
    fn cardinality_and_label_bounds_drop_new_series() {
        let recorder = InMemoryRecorder::new(MetricsPolicy {
            max_series: 1,
            max_labels_per_series: 1,
            max_field_length: 16,
        });
        let metadata = Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        recorder
            .register_counter(&Key::from_name("accepted"), &metadata)
            .increment(1);
        recorder
            .register_counter(&Key::from_name("overflow"), &metadata)
            .increment(1);
        recorder
            .register_counter(
                &Key::from_parts(
                    "labels",
                    vec![
                        metrics::Label::new("first", "1"),
                        metrics::Label::new("second", "2"),
                    ],
                ),
                &metadata,
            )
            .increment(1);

        assert_eq!(recorder.snapshot().len(), 1);
        assert_eq!(recorder.dropped_series(), 2);
    }

    #[test]
    fn fanout_records_to_memory_and_exporter() {
        let primary = Arc::new(InMemoryRecorder::default());
        let exporter = Arc::new(InMemoryRecorder::default());
        let fanout = FanoutRecorder::new(primary.clone(), exporter.clone());
        let metadata = Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        fanout
            .register_counter(&Key::from_name("fanout"), &metadata)
            .increment(2);

        assert_eq!(primary.snapshot(), exporter.snapshot());
    }
}
