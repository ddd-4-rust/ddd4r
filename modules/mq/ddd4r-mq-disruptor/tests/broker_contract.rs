//! End-to-end contract for the bounded local broker.

use std::sync::Arc;
use std::time::Duration;

use ddd4r_mq_core::testkit::CountingHandler;
use ddd4r_mq_core::{
    ConsumerEngine, HandlerOutcome, InMemoryIdempotencyStore, MessagePublisher, MqEvent,
    RetryPolicy, Subscription, TagExpression, TopicAddress,
};
use ddd4r_mq_disruptor::DisruptorBroker;

fn event(tag: &str) -> MqEvent {
    MqEvent::new(
        "order.created",
        TopicAddress::new(None, "orders", Some(tag.to_owned()), ".").unwrap(),
        vec![1, 2, 3],
    )
    .unwrap()
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..100 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition was not satisfied before timeout");
}

#[tokio::test]
async fn publish_routes_to_matching_active_subscribers() {
    let broker = DisruptorBroker::new(8).unwrap();
    let handler = CountingHandler::success(HandlerOutcome::Ack);
    let subscription = broker
        .subscribe(
            "orders-paid",
            ["order.created".to_owned()],
            TagExpression::parse("paid"),
            ConsumerEngine::new(
                Arc::new(InMemoryIdempotencyStore::default()),
                None,
                RetryPolicy::default(),
            ),
            Arc::new(handler.clone()),
        )
        .unwrap();
    broker.start().await.unwrap();
    broker.publish(event("paid")).await.unwrap();
    broker.publish(event("cancelled")).await.unwrap();
    wait_until(|| handler.call_count() == 1).await;

    subscription.close().await.unwrap();
    broker.publish(event("paid")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(handler.call_count(), 1);
    broker.close().await.unwrap();
}

#[tokio::test]
async fn handler_failure_retries_to_limit_then_records_dead_letter() {
    let broker = DisruptorBroker::new(2).unwrap();
    let handler = CountingHandler::failure("boom");
    broker
        .subscribe(
            "orders",
            ["*".to_owned()],
            TagExpression::parse("*"),
            ConsumerEngine::new(
                Arc::new(InMemoryIdempotencyStore::default()),
                None,
                RetryPolicy { max_attempts: 3 },
            ),
            Arc::new(handler.clone()),
        )
        .unwrap();
    broker.start().await.unwrap();
    let event = event("paid");
    let message_id = event.message_id;
    broker.publish(event).await.unwrap();
    wait_until(|| handler.call_count() == 3).await;
    wait_until(|| {
        futures::executor::block_on(broker.dead_letters())
            .iter()
            .any(|event| event.message_id == message_id)
    })
    .await;
    assert_eq!(broker.dead_letters().await[0].delivery_attempt, 3);
    broker.close().await.unwrap();
}

#[tokio::test]
async fn publish_fails_before_start_and_after_close() {
    let broker = DisruptorBroker::new(1).unwrap();
    assert!(broker.publish(event("paid")).await.is_err());
    broker.start().await.unwrap();
    broker.close().await.unwrap();
    assert!(broker.publish(event("paid")).await.is_err());
}

#[tokio::test]
async fn duplicate_subscription_is_rejected_without_replacing_original() {
    let broker = DisruptorBroker::new(2).unwrap();
    let original = CountingHandler::success(HandlerOutcome::Ack);
    let replacement = CountingHandler::success(HandlerOutcome::Ack);
    let engine = || {
        ConsumerEngine::new(
            Arc::new(InMemoryIdempotencyStore::default()),
            None,
            RetryPolicy::default(),
        )
    };

    broker
        .subscribe(
            "orders",
            ["*".to_owned()],
            TagExpression::parse("*"),
            engine(),
            Arc::new(original.clone()),
        )
        .unwrap();
    assert!(
        broker
            .subscribe(
                "orders",
                ["*".to_owned()],
                TagExpression::parse("*"),
                engine(),
                Arc::new(replacement.clone()),
            )
            .is_err()
    );

    broker.start().await.unwrap();
    broker.publish(event("paid")).await.unwrap();
    wait_until(|| original.call_count() == 1).await;
    assert_eq!(replacement.call_count(), 0);
    broker.close().await.unwrap();
}
