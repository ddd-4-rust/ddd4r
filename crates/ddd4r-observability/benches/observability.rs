//! Microbenchmarks for default observability primitives.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use ddd4r_observability::{HealthRegistry, HealthStatus, InMemoryRecorder, MetricsPolicy};
use metrics::{Key, KeyName, Label, Metadata, Recorder, SharedString, Unit};

fn benchmark_health_report(criterion: &mut Criterion) {
    let registry = HealthRegistry::default();
    registry
        .set("database", HealthStatus::Up, true, None)
        .expect("valid component");
    registry
        .set("redis", HealthStatus::Up, true, None)
        .expect("valid component");

    criterion.bench_function("health_report_three_components", |bencher| {
        bencher.iter(|| black_box(registry.report()));
    });
}

fn benchmark_metric_recording(criterion: &mut Criterion) {
    let recorder = InMemoryRecorder::default();
    let metadata = Metadata::new(
        "ddd4r_benchmark",
        metrics::Level::INFO,
        Some("ddd4r-observability"),
    );
    let counter = recorder.register_counter(&Key::from_name("requests_total"), &metadata);
    let gauge = recorder.register_gauge(&Key::from_name("inflight_requests"), &metadata);
    let histogram = recorder.register_histogram(&Key::from_name("request_seconds"), &metadata);

    criterion.bench_function("record_counter_gauge_histogram", |bencher| {
        bencher.iter(|| {
            counter.increment(black_box(1));
            gauge.set(black_box(1.0));
            histogram.record(black_box(0.001));
        });
    });
}

fn benchmark_metrics_policy(criterion: &mut Criterion) {
    let policy = MetricsPolicy::default();
    let key = Key::from_parts(
        "http_server_request_duration_seconds",
        vec![
            Label::new("method", "GET"),
            Label::new("route", "/orders/:id"),
            Label::new("status", "200"),
        ],
    );

    criterion.bench_function("validate_metric_identity_and_labels", |bencher| {
        bencher.iter(|| black_box(policy.allows(black_box(&key))));
    });
}

fn recorder_trait_contract(recorder: &dyn Recorder) {
    recorder.describe_counter(
        KeyName::from("requests_total"),
        Some(Unit::Count),
        SharedString::from("request count"),
    );
}

fn benchmarks(criterion: &mut Criterion) {
    let recorder = InMemoryRecorder::default();
    recorder_trait_contract(&recorder);
    benchmark_health_report(criterion);
    benchmark_metric_recording(criterion);
    benchmark_metrics_policy(criterion);
}

criterion_group!(observability_benches, benchmarks);
criterion_main!(observability_benches);
