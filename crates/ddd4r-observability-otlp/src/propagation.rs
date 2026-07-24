//! W3C Trace Context and Baggage propagation helpers.

use opentelemetry::Context;
use opentelemetry::propagation::{
    Extractor, Injector, TextMapCompositePropagator, TextMapPropagator,
};
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{OtlpError, OtlpResult};

/// Stateless W3C propagation facade used by HTTP and messaging adapters.
#[derive(Debug, Clone, Copy, Default)]
pub struct W3cPropagation;

impl W3cPropagation {
    /// Extracts Trace Context and Baggage from an inbound carrier.
    pub fn extract(carrier: &dyn Extractor) -> Context {
        propagator().extract(carrier)
    }

    /// Injects an explicit OpenTelemetry context into an outbound carrier.
    pub fn inject(context: &Context, carrier: &mut dyn Injector) {
        propagator().inject_context(context, carrier);
    }

    /// Sets the remote parent on a newly created `tracing` span.
    pub fn set_parent(span: &Span, carrier: &dyn Extractor) -> OtlpResult<()> {
        span.set_parent(Self::extract(carrier))
            .map_err(|error| OtlpError::ParentContext(error.to_string()))
    }

    /// Injects the current `tracing` span context into an outbound carrier.
    pub fn inject_current(carrier: &mut dyn Injector) {
        Self::inject(&Span::current().context(), carrier);
    }
}

fn propagator() -> TextMapCompositePropagator {
    TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ])
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use opentelemetry::baggage::BaggageExt;
    use opentelemetry::trace::TraceContextExt;

    use super::*;

    #[test]
    fn round_trips_w3c_trace_context_and_baggage() {
        let inbound = HashMap::from([
            (
                "traceparent".to_owned(),
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
            ),
            ("baggage".to_owned(), "tenant=acme".to_owned()),
        ]);
        let context = W3cPropagation::extract(&inbound);
        let mut outbound = HashMap::new();
        W3cPropagation::inject(&context, &mut outbound);

        assert!(context.span().span_context().is_remote());
        assert_eq!(
            context
                .baggage()
                .get("tenant")
                .map(opentelemetry::StringValue::as_str),
            Some("acme")
        );
        assert_eq!(outbound.get("traceparent"), inbound.get("traceparent"));
        assert_eq!(outbound.get("baggage"), inbound.get("baggage"));
    }
}
