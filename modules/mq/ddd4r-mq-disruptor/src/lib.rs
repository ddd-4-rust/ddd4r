//! Bounded Tokio event-bus migration adapter for `ddd4j-mq-disruptor`.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use ddd4r_mq_core::{
    BrokerType, ConsumeResult, ConsumerEngine, Delivery, InMemoryAcknowledgment, MessageHandler,
    MessagePublisher, MqError, MqEvent, MqResult, PublishReceipt, Subscription, TagExpression,
};
use futures::future::BoxFuture;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-mq-disruptor",
    rust_package: "ddd4r-mq-disruptor",
    group: "mq",
    maturity: ModuleMaturity::InProgress,
};

#[derive(Clone)]
struct Subscriber {
    supports: BTreeSet<String>,
    tags: TagExpression,
    handler: Arc<dyn MessageHandler>,
    engine: ConsumerEngine,
    active: Arc<AtomicBool>,
}

impl Subscriber {
    fn supports(&self, event: &MqEvent) -> bool {
        self.supports.contains("*") || self.supports.contains(&event.event_type)
    }
}

struct Inner {
    sender: mpsc::Sender<MqEvent>,
    receiver: Mutex<Option<mpsc::Receiver<MqEvent>>>,
    subscribers: DashMap<String, Subscriber>,
    dead_letters: Mutex<Vec<MqEvent>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    sequence: AtomicU64,
    started: AtomicBool,
    closed: AtomicBool,
}

/// Bounded process-local broker.
#[derive(Clone)]
pub struct DisruptorBroker {
    inner: Arc<Inner>,
}

impl DisruptorBroker {
    /// Creates a broker with strict positive bounded capacity.
    pub fn new(capacity: usize) -> MqResult<Self> {
        if capacity == 0 {
            return Err(MqError::InvalidInput {
                message: "disruptor capacity must be positive".to_owned(),
            });
        }
        let (sender, receiver) = mpsc::channel(capacity);
        Ok(Self {
            inner: Arc::new(Inner {
                sender,
                receiver: Mutex::new(Some(receiver)),
                subscribers: DashMap::new(),
                dead_letters: Mutex::new(Vec::new()),
                worker: Mutex::new(None),
                sequence: AtomicU64::new(0),
                started: AtomicBool::new(false),
                closed: AtomicBool::new(false),
            }),
        })
    }

    /// Registers a listener before or after the worker starts.
    pub fn subscribe(
        &self,
        id: impl Into<String>,
        supports: impl IntoIterator<Item = String>,
        tags: TagExpression,
        engine: ConsumerEngine,
        handler: Arc<dyn MessageHandler>,
    ) -> MqResult<DisruptorSubscription> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(MqError::InvalidInput {
                message: "subscription id must not be blank".to_owned(),
            });
        }
        let active = Arc::new(AtomicBool::new(true));
        let subscriber = Subscriber {
            supports: supports.into_iter().collect(),
            tags,
            handler,
            engine,
            active: active.clone(),
        };
        match self.inner.subscribers.entry(id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(subscriber);
            }
            Entry::Occupied(_) => {
                return Err(MqError::InvalidInput {
                    message: format!("duplicate subscription id: {id}"),
                });
            }
        }
        Ok(DisruptorSubscription { id, active })
    }

    /// Starts the single bounded dispatch loop exactly once.
    pub async fn start(&self) -> MqResult<()> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(Self::broker_error("start", "broker is closed"));
        }
        if self
            .inner
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        let receiver = self
            .inner
            .receiver
            .lock()
            .await
            .take()
            .ok_or_else(|| Self::broker_error("start", "receiver was already taken"))?;
        let broker = self.clone();
        let worker = tokio::spawn(async move {
            broker.run(receiver).await;
        });
        *self.inner.worker.lock().await = Some(worker);
        Ok(())
    }

    async fn run(&self, mut receiver: mpsc::Receiver<MqEvent>) {
        while let Some(event) = receiver.recv().await {
            let subscribers = self
                .inner
                .subscribers
                .iter()
                .filter(|entry| entry.active.load(Ordering::Acquire) && entry.supports(&event))
                .map(|entry| entry.value().clone())
                .collect::<Vec<_>>();
            for subscriber in subscribers {
                self.deliver(&subscriber, event.clone()).await;
            }
        }
    }

    async fn deliver(&self, subscriber: &Subscriber, mut event: MqEvent) {
        loop {
            let sequence = self
                .inner
                .sequence
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            let acknowledgment = InMemoryAcknowledgment::new(
                sequence,
                event.message_id.to_string(),
                event.correlation_id.clone(),
                BrokerType::Disruptor,
                Arc::new(|_| Ok(())),
            );
            let result = subscriber
                .engine
                .consume(
                    Delivery {
                        event: event.clone(),
                        acknowledgment: Arc::new(acknowledgment),
                    },
                    &subscriber.tags,
                    subscriber.handler.as_ref(),
                )
                .await;
            match result {
                Ok(ConsumeResult::Requeued) => {
                    event.delivery_attempt = event.delivery_attempt.saturating_add(1);
                }
                Ok(ConsumeResult::DeadLettered) | Err(_) => {
                    self.inner.dead_letters.lock().await.push(event);
                    break;
                }
                Ok(_) => break,
            }
        }
    }

    /// Returns a snapshot of terminally failed messages.
    pub async fn dead_letters(&self) -> Vec<MqEvent> {
        self.inner.dead_letters.lock().await.clone()
    }

    /// Stops the worker. Queued messages not yet delivered remain unprocessed.
    pub async fn close(&self) -> MqResult<()> {
        if self.inner.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        if let Some(worker) = self.inner.worker.lock().await.take() {
            worker.abort();
            let _ = worker.await;
        }
        Ok(())
    }

    fn broker_error(operation: &'static str, message: impl Into<String>) -> MqError {
        MqError::Broker {
            broker: BrokerType::Disruptor.as_str().to_owned(),
            operation,
            message: message.into(),
        }
    }
}

impl MessagePublisher for DisruptorBroker {
    fn publish(&self, event: MqEvent) -> BoxFuture<'_, MqResult<PublishReceipt>> {
        Box::pin(async move {
            if !self.inner.started.load(Ordering::Acquire) {
                return Err(Self::broker_error("publish", "broker is not started"));
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return Err(Self::broker_error("publish", "broker is closed"));
            }
            let receipt = PublishReceipt {
                message_id: event.message_id,
                broker_message_id: Some(event.message_id.to_string()),
                broker: BrokerType::Disruptor,
                destination: event.destination.physical_name(),
            };
            self.inner
                .sender
                .send(event)
                .await
                .map_err(|_| Self::broker_error("publish", "dispatch worker stopped"))?;
            Ok(receipt)
        })
    }
}

/// Handle that disables one listener without stopping the broker.
#[derive(Clone)]
pub struct DisruptorSubscription {
    id: String,
    active: Arc<AtomicBool>,
}

impl Subscription for DisruptorSubscription {
    fn id(&self) -> &str {
        &self.id
    }

    fn close(&self) -> BoxFuture<'_, MqResult<()>> {
        Box::pin(async move {
            self.active.store(false, Ordering::Release);
            Ok(())
        })
    }
}
