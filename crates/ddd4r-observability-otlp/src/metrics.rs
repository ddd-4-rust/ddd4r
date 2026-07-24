//! Bridge from the Rust `metrics` facade to OpenTelemetry metrics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use ddd4r_observability::MetricsPolicy;
use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{
    Counter as OtelCounter, Gauge as OtelGauge, Histogram as OtelHistogram, Meter,
};

/// A bounded `metrics::Recorder` backed by one OpenTelemetry `Meter`.
pub struct OpenTelemetryMetricsRecorder {
    meter: Meter,
    policy: MetricsPolicy,
    series: AtomicUsize,
    dropped_series: AtomicU64,
    counter_descriptions: RwLock<BTreeMap<String, InstrumentDescriptor>>,
    gauge_descriptions: RwLock<BTreeMap<String, InstrumentDescriptor>>,
    histogram_descriptions: RwLock<BTreeMap<String, InstrumentDescriptor>>,
    counter_instruments: RwLock<BTreeMap<String, OtelCounter<u64>>>,
    gauge_instruments: RwLock<BTreeMap<String, OtelGauge<f64>>>,
    histogram_instruments: RwLock<BTreeMap<String, OtelHistogram<f64>>>,
    counters: RwLock<BTreeMap<String, Arc<OtelCounterHandle>>>,
    gauges: RwLock<BTreeMap<String, Arc<OtelGaugeHandle>>>,
    histograms: RwLock<BTreeMap<String, Arc<OtelHistogramHandle>>>,
}

impl std::fmt::Debug for OpenTelemetryMetricsRecorder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenTelemetryMetricsRecorder")
            .field("policy", &self.policy)
            .field("series", &self.series.load(Ordering::Acquire))
            .field("dropped_series", &self.dropped_series())
            .finish_non_exhaustive()
    }
}

impl OpenTelemetryMetricsRecorder {
    /// Creates a recorder scoped to the supplied OpenTelemetry meter.
    pub fn new(meter: Meter, policy: MetricsPolicy) -> Self {
        Self {
            meter,
            policy,
            series: AtomicUsize::new(0),
            dropped_series: AtomicU64::new(0),
            counter_descriptions: RwLock::new(BTreeMap::new()),
            gauge_descriptions: RwLock::new(BTreeMap::new()),
            histogram_descriptions: RwLock::new(BTreeMap::new()),
            counter_instruments: RwLock::new(BTreeMap::new()),
            gauge_instruments: RwLock::new(BTreeMap::new()),
            histogram_instruments: RwLock::new(BTreeMap::new()),
            counters: RwLock::new(BTreeMap::new()),
            gauges: RwLock::new(BTreeMap::new()),
            histograms: RwLock::new(BTreeMap::new()),
        }
    }

    /// Returns the number of metric series rejected by local bounds.
    pub fn dropped_series(&self) -> u64 {
        self.dropped_series.load(Ordering::Acquire)
    }

    fn accepts(&self, key: &Key) -> bool {
        self.policy.allows(key) && valid_otel_instrument_name(key.name())
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

impl Recorder for OpenTelemetryMetricsRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        write_unpoisoned(&self.counter_descriptions).insert(
            key.as_str().to_owned(),
            InstrumentDescriptor::new(unit, &description),
        );
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        write_unpoisoned(&self.gauge_descriptions).insert(
            key.as_str().to_owned(),
            InstrumentDescriptor::new(unit, &description),
        );
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        write_unpoisoned(&self.histogram_descriptions).insert(
            key.as_str().to_owned(),
            InstrumentDescriptor::new(unit, &description),
        );
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        if !self.accepts(key) {
            self.reject_series();
            return Counter::noop();
        }
        let identity = metric_identity(key);
        let mut handles = write_unpoisoned(&self.counters);
        if let Some(handle) = handles.get(&identity) {
            return Counter::from_arc(handle.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Counter::noop();
        }
        let instrument = {
            let mut instruments = write_unpoisoned(&self.counter_instruments);
            instruments
                .entry(key.name().to_owned())
                .or_insert_with(|| {
                    let descriptor = read_unpoisoned(&self.counter_descriptions)
                        .get(key.name())
                        .cloned();
                    let builder = self.meter.u64_counter(key.name().to_owned());
                    match descriptor {
                        Some(value) => value.counter(builder),
                        None => builder.build(),
                    }
                })
                .clone()
        };
        let handle = Arc::new(OtelCounterHandle {
            instrument,
            attributes: metric_attributes(key),
            current: AtomicU64::new(0),
        });
        handles.insert(identity, handle.clone());
        Counter::from_arc(handle)
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        if !self.accepts(key) {
            self.reject_series();
            return Gauge::noop();
        }
        let identity = metric_identity(key);
        let mut handles = write_unpoisoned(&self.gauges);
        if let Some(handle) = handles.get(&identity) {
            return Gauge::from_arc(handle.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Gauge::noop();
        }
        let instrument = {
            let mut instruments = write_unpoisoned(&self.gauge_instruments);
            instruments
                .entry(key.name().to_owned())
                .or_insert_with(|| {
                    let descriptor = read_unpoisoned(&self.gauge_descriptions)
                        .get(key.name())
                        .cloned();
                    let builder = self.meter.f64_gauge(key.name().to_owned());
                    match descriptor {
                        Some(value) => value.gauge(builder),
                        None => builder.build(),
                    }
                })
                .clone()
        };
        let handle = Arc::new(OtelGaugeHandle {
            instrument,
            attributes: metric_attributes(key),
            current: AtomicU64::new(0.0_f64.to_bits()),
        });
        handles.insert(identity, handle.clone());
        Gauge::from_arc(handle)
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        if !self.accepts(key) {
            self.reject_series();
            return Histogram::noop();
        }
        let identity = metric_identity(key);
        let mut handles = write_unpoisoned(&self.histograms);
        if let Some(handle) = handles.get(&identity) {
            return Histogram::from_arc(handle.clone());
        }
        if !self.reserve_series() {
            self.reject_series();
            return Histogram::noop();
        }
        let instrument = {
            let mut instruments = write_unpoisoned(&self.histogram_instruments);
            instruments
                .entry(key.name().to_owned())
                .or_insert_with(|| {
                    let descriptor = read_unpoisoned(&self.histogram_descriptions)
                        .get(key.name())
                        .cloned();
                    let builder = self.meter.f64_histogram(key.name().to_owned());
                    match descriptor {
                        Some(value) => value.histogram(builder),
                        None => builder.build(),
                    }
                })
                .clone()
        };
        let handle = Arc::new(OtelHistogramHandle {
            instrument,
            attributes: metric_attributes(key),
        });
        handles.insert(identity, handle.clone());
        Histogram::from_arc(handle)
    }
}

#[derive(Debug, Clone)]
struct InstrumentDescriptor {
    unit: Option<&'static str>,
    description: String,
}

impl InstrumentDescriptor {
    fn new(unit: Option<Unit>, description: &SharedString) -> Self {
        Self {
            unit: unit.map(otel_unit),
            description: description.to_string(),
        }
    }

    fn counter(
        &self,
        mut builder: opentelemetry::metrics::InstrumentBuilder<'_, OtelCounter<u64>>,
    ) -> OtelCounter<u64> {
        builder = builder.with_description(self.description.clone());
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }

    fn gauge(
        &self,
        mut builder: opentelemetry::metrics::InstrumentBuilder<'_, OtelGauge<f64>>,
    ) -> OtelGauge<f64> {
        builder = builder.with_description(self.description.clone());
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }

    fn histogram(
        &self,
        mut builder: opentelemetry::metrics::HistogramBuilder<'_, OtelHistogram<f64>>,
    ) -> OtelHistogram<f64> {
        builder = builder.with_description(self.description.clone());
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
}

struct OtelCounterHandle {
    instrument: OtelCounter<u64>,
    attributes: Vec<KeyValue>,
    current: AtomicU64,
}

impl CounterFn for OtelCounterHandle {
    fn increment(&self, value: u64) {
        let _ = self
            .current
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_add(value))
            });
        self.instrument.add(value, &self.attributes);
    }

    fn absolute(&self, value: u64) {
        let mut current = self.current.load(Ordering::Acquire);
        while value > current {
            match self.current.compare_exchange_weak(
                current,
                value,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.instrument.add(value - current, &self.attributes);
                    return;
                }
                Err(observed) => current = observed,
            }
        }
    }
}

struct OtelGaugeHandle {
    instrument: OtelGauge<f64>,
    attributes: Vec<KeyValue>,
    current: AtomicU64,
}

impl OtelGaugeHandle {
    fn update(&self, operation: impl Fn(f64) -> f64) {
        let mut current = self.current.load(Ordering::Acquire);
        loop {
            let value = operation(f64::from_bits(current));
            match self.current.compare_exchange_weak(
                current,
                value.to_bits(),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.instrument.record(value, &self.attributes);
                    return;
                }
                Err(observed) => current = observed,
            }
        }
    }
}

impl GaugeFn for OtelGaugeHandle {
    fn increment(&self, value: f64) {
        self.update(|current| current + value);
    }

    fn decrement(&self, value: f64) {
        self.update(|current| current - value);
    }

    fn set(&self, value: f64) {
        self.current.store(value.to_bits(), Ordering::Release);
        self.instrument.record(value, &self.attributes);
    }
}

struct OtelHistogramHandle {
    instrument: OtelHistogram<f64>,
    attributes: Vec<KeyValue>,
}

impl HistogramFn for OtelHistogramHandle {
    fn record(&self, value: f64) {
        self.instrument.record(value, &self.attributes);
    }
}

fn metric_attributes(key: &Key) -> Vec<KeyValue> {
    key.labels()
        .map(|label| KeyValue::new(label.key().to_owned(), label.value().to_owned()))
        .collect()
}

fn metric_identity(key: &Key) -> String {
    let mut labels = key
        .labels()
        .map(|label| (label.key(), label.value()))
        .collect::<Vec<_>>();
    labels.sort_unstable();
    let labels = labels
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}{{{labels}}}", key.name())
}

fn valid_otel_instrument_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'/'))
}

fn otel_unit(unit: Unit) -> &'static str {
    match unit {
        Unit::Microseconds => "us",
        _ => unit.as_canonical_label(),
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

#[cfg(test)]
mod tests {
    use opentelemetry::metrics::MeterProvider;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider};

    use super::*;

    #[test]
    fn exports_metrics_facade_measurements_and_enforces_bounds() {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter.clone())
            .build();
        let recorder = OpenTelemetryMetricsRecorder::new(
            provider.meter("ddd4r-test"),
            MetricsPolicy {
                max_series: 1,
                ..MetricsPolicy::default()
            },
        );
        let metadata = Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        recorder
            .register_counter(&Key::from_name("ddd4r.requests"), &metadata)
            .increment(3);
        recorder
            .register_counter(&Key::from_name("ddd4r.overflow"), &metadata)
            .increment(1);
        provider.force_flush().expect("flush metrics");

        assert_eq!(recorder.dropped_series(), 1);
        assert!(!exporter.get_finished_metrics().expect("metrics").is_empty());
        provider.shutdown().expect("shutdown metrics");
    }
}
