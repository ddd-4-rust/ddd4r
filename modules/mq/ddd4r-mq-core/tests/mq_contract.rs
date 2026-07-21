//! Executable contract for the frozen ddd4j MQ core semantics.

use std::sync::Arc;

use ddd4r_mq_core::testkit::{CountingHandler, acknowledgment};
use ddd4r_mq_core::{
    AckCommand, AckState, Acknowledgment, BrokerType, ConsumeResult, ConsumerEngine, Delivery,
    HandlerOutcome, InMemoryIdempotencyStore, MqEvent, PartitionKeyStrategy, RetryPolicy,
    TagExpression, TopicAddress,
};

fn event(tag: Option<&str>, attempt: u32) -> MqEvent {
    let mut event = MqEvent::new(
        "order.created",
        TopicAddress::new(
            Some("uat".to_owned()),
            "orders",
            tag.map(str::to_owned),
            ".",
        )
        .unwrap(),
        br#"{"order_id":"O-1"}"#.to_vec(),
    )
    .unwrap();
    event.tenant_id = Some("tenant-a".to_owned());
    event.delivery_attempt = attempt;
    event
}

#[test]
fn broker_routing_headers_and_partition_keys_are_stable() {
    assert_eq!(BrokerType::parse("redisStream"), BrokerType::RedisStream);
    assert_eq!(BrokerType::parse("local-disruptor"), BrokerType::Disruptor);
    assert_eq!(BrokerType::parse("unknown"), BrokerType::None);

    let event = event(Some("paid"), 1);
    assert_eq!(event.destination.physical_name(), "uat.orders.paid");
    assert_eq!(
        event.partition_key(PartitionKeyStrategy::TagTenant),
        Some("paid|tenant-a".to_owned())
    );
}

#[test]
fn tag_expression_matches_and_escapes_sql92_selectors() {
    let tags = TagExpression::parse("paid || shipped -cancelled");
    assert!(tags.matches(Some("paid")));
    assert!(tags.matches(None));
    assert!(!tags.matches(Some("cancelled")));
    assert_eq!(
        TagExpression::parse("* -can'celled").to_sql92_selector("ddd4jTag"),
        Some("(1=1 AND (ddd4jTag <> 'can''celled' OR ddd4jTag IS NULL))".to_owned())
    );
}

#[tokio::test]
async fn acknowledgment_is_exactly_once_and_rolls_back_state_on_adapter_failure() {
    let (ack, commands) = acknowledgment(BrokerType::Kafka, "message-1");
    ack.ack().await.unwrap();
    assert_eq!(ack.state(), AckState::Acknowledged);
    assert!(ack.nack(true).await.is_err());
    assert_eq!(
        *commands.lock().unwrap(),
        vec![AckCommand::Ack { multiple: false }]
    );
}

#[tokio::test]
async fn successful_delivery_is_idempotent_across_redelivery() {
    let idempotency = Arc::new(InMemoryIdempotencyStore::default());
    let engine = ConsumerEngine::new(idempotency, None, RetryPolicy::default());
    let handler = CountingHandler::success(HandlerOutcome::Ack);
    let event = event(Some("paid"), 1);

    let (first_ack, _) = acknowledgment(BrokerType::Rabbit, event.message_id.to_string());
    let first = engine
        .consume(
            Delivery {
                event: event.clone(),
                acknowledgment: Arc::new(first_ack),
            },
            &TagExpression::parse("paid"),
            &handler,
        )
        .await
        .unwrap();
    assert_eq!(first, ConsumeResult::Acknowledged);

    let (duplicate_ack, _) = acknowledgment(BrokerType::Rabbit, event.message_id.to_string());
    let duplicate = engine
        .consume(
            Delivery {
                event,
                acknowledgment: Arc::new(duplicate_ack),
            },
            &TagExpression::parse("paid"),
            &handler,
        )
        .await
        .unwrap();
    assert_eq!(duplicate, ConsumeResult::Duplicate);
    assert_eq!(handler.call_count(), 1);
}

#[tokio::test]
async fn handler_failures_requeue_then_dead_letter_at_the_attempt_limit() {
    let idempotency = Arc::new(InMemoryIdempotencyStore::default());
    let engine = ConsumerEngine::new(idempotency, None, RetryPolicy { max_attempts: 3 });
    let handler = CountingHandler::failure("boom");

    let retry_event = event(Some("paid"), 2);
    let (retry_ack, _) = acknowledgment(BrokerType::Nats, retry_event.message_id.to_string());
    let retry = engine
        .consume(
            Delivery {
                event: retry_event,
                acknowledgment: Arc::new(retry_ack.clone()),
            },
            &TagExpression::parse("*"),
            &handler,
        )
        .await
        .unwrap();
    assert_eq!(retry, ConsumeResult::Requeued);
    assert_eq!(retry_ack.state(), AckState::Requeued);

    let terminal_event = event(Some("paid"), 3);
    let (terminal_ack, _) = acknowledgment(BrokerType::Nats, terminal_event.message_id.to_string());
    let terminal = engine
        .consume(
            Delivery {
                event: terminal_event,
                acknowledgment: Arc::new(terminal_ack.clone()),
            },
            &TagExpression::parse("*"),
            &handler,
        )
        .await
        .unwrap();
    assert_eq!(terminal, ConsumeResult::DeadLettered);
    assert_eq!(terminal_ack.state(), AckState::Discarded);
}

#[tokio::test]
async fn tag_filtering_acknowledges_without_invoking_handler() {
    let engine = ConsumerEngine::new(
        Arc::new(InMemoryIdempotencyStore::default()),
        None,
        RetryPolicy::default(),
    );
    let handler = CountingHandler::success(HandlerOutcome::Ack);
    let event = event(Some("cancelled"), 1);
    let (ack, _) = acknowledgment(BrokerType::Sqs, event.message_id.to_string());
    let result = engine
        .consume(
            Delivery {
                event,
                acknowledgment: Arc::new(ack.clone()),
            },
            &TagExpression::parse("paid -cancelled"),
            &handler,
        )
        .await
        .unwrap();
    assert_eq!(result, ConsumeResult::Filtered);
    assert_eq!(handler.call_count(), 0);
    assert_eq!(ack.state(), AckState::Acknowledged);
}
