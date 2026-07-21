//! CQRS projection compatibility contracts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ddd4r_core::DddResult;
use ddd4r_core::event::EventEnvelope;
use ddd4r_core::projection::{
    EventChunk, EventChunkReader, Projection, ProjectionPositionRepository, ProjectionRunner,
    ProjectionService,
};
use futures::future::BoxFuture;
use serde_json::{Map, Value};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Default)]
struct Positions(Arc<Mutex<HashMap<String, u64>>>);

impl ProjectionPositionRepository for Positions {
    fn load<'a>(&'a self, projection: &'a str) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move { Ok(*self.0.lock().unwrap().get(projection).unwrap_or(&0)) })
    }

    fn save<'a>(&'a self, projection: &'a str, position: u64) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .insert(projection.to_owned(), position);
            Ok(())
        })
    }
}

struct Reader {
    event: EventEnvelope,
}

impl EventChunkReader for Reader {
    fn read<'a>(
        &'a self,
        stream_id: &'a str,
        from: u64,
        limit: usize,
        event_types: &'a [&'a str],
    ) -> BoxFuture<'a, DddResult<EventChunk>> {
        Box::pin(async move {
            assert_eq!(stream_id, "orders");
            assert_eq!(from, 0);
            assert_eq!(limit, 10);
            assert_eq!(event_types, ["OrderCreated"]);
            Ok(EventChunk {
                from,
                next: 1,
                events: vec![self.event.clone()],
            })
        })
    }
}

#[derive(Default)]
struct OrderProjection(Arc<Mutex<Vec<String>>>);

impl Projection for OrderProjection {
    fn name(&self) -> &'static str {
        "order-view"
    }

    fn stream_id(&self) -> &'static str {
        "orders"
    }

    fn event_types(&self) -> &'static [&'static str] {
        &["OrderCreated"]
    }

    fn apply<'a>(&'a self, event: &'a EventEnvelope) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(event.event_type.clone());
            Ok(())
        })
    }
}

#[tokio::test]
async fn projection_service_and_runner_preserve_stream_checkpoint_semantics() {
    let positions = Positions::default();
    let service = ProjectionService::new(positions.clone());
    let position = service
        .update_projection_position("orders", 7)
        .await
        .unwrap();
    assert_eq!(position.next_event_number, 7);
    assert_eq!(position.with_next_event_number(8).next_event_number, 8);
    service.reset_projection_position("orders").await.unwrap();

    let projection = OrderProjection::default();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../behavior-fixtures/events/order-created.json"
    ))
    .unwrap();
    let reader = Reader {
        event: EventEnvelope {
            event_id: Uuid::now_v7(),
            aggregate_type: fixture["aggregateType"].as_str().unwrap().to_owned(),
            aggregate_id: fixture["aggregateId"].as_str().unwrap().to_owned(),
            aggregate_version: fixture["aggregateVersion"].as_u64().unwrap(),
            event_type: fixture["eventType"].as_str().unwrap().to_owned(),
            event_version: u32::try_from(fixture["eventVersion"].as_u64().unwrap()).unwrap(),
            occurred_at: OffsetDateTime::now_utc(),
            tenant_id: fixture["tenantId"].as_str().map(str::to_owned),
            correlation_id: None,
            causation_id: None,
            support_keys: fixture["supportKeys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect(),
            payload: fixture["payload"].clone(),
            metadata: Map::new(),
        },
    };
    let runner = ProjectionRunner::new(reader, positions.clone(), 10);
    assert!(runner.run_once(&projection).await.unwrap());
    assert_eq!(service.read_projection_position("orders").await.unwrap(), 1);
    assert_eq!(projection.0.lock().unwrap().as_slice(), ["OrderCreated"]);
}
