//! Message envelope, configuration and delivery values.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{Acknowledgment, MqError, MqResult};

/// Standard header keys shared by all adapters.
pub struct MessageHeaders;

impl MessageHeaders {
    /// Message identifier.
    pub const MESSAGE_ID: &'static str = "ddd4j.message.id";
    /// Correlation identifier.
    pub const CORRELATION_ID: &'static str = "ddd4j.correlation.id";
    /// Causation identifier.
    pub const CAUSATION_ID: &'static str = "ddd4j.causation.id";
    /// Tenant identifier.
    pub const TENANT_ID: &'static str = "ddd4j.tenant.id";
    /// Broker type.
    pub const BROKER_TYPE: &'static str = "ddd4j.broker.type";
    /// Destination topic.
    pub const DESTINATION_TOPIC: &'static str = "ddd4j.destination.topic";
    /// Destination tag.
    pub const DESTINATION_TAG: &'static str = "ddd4j.destination.tag";
    /// Destination namespace.
    pub const DESTINATION_NAMESPACE: &'static str = "ddd4j.destination.namespace";
    /// Delivery attempt, starting from one.
    pub const DELIVERY_ATTEMPT: &'static str = "ddd4r.delivery.attempt";
}

/// Supported broker families.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrokerType {
    /// Messaging disabled or unknown.
    #[default]
    None,
    /// Process-local bounded queue.
    Disruptor,
    /// `RabbitMQ` / AMQP.
    Rabbit,
    /// Apache Kafka.
    Kafka,
    /// Apache `RocketMQ`.
    Rocket,
    /// Apache Pulsar.
    Pulsar,
    /// Redis Streams.
    RedisStream,
    /// `ActiveMQ` / Artemis.
    ActiveMq,
    /// NATS `JetStream`.
    Nats,
    /// MQTT.
    Mqtt,
    /// mica-mqtt migration entry.
    MqttMica,
    /// Alibaba ONS.
    Ons,
    /// Tencent TDMQ.
    Tdmq,
    /// AWS SQS.
    Sqs,
}

impl BrokerType {
    /// Parses canonical and frozen legacy configuration names.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "disruptor" | "local" | "local-disruptor" => Self::Disruptor,
            "rabbit" | "rabbitmq" => Self::Rabbit,
            "kafka" => Self::Kafka,
            "rocket" | "rocketmq" => Self::Rocket,
            "pulsar" => Self::Pulsar,
            "redis" | "redis-stream" | "redisstream" => Self::RedisStream,
            "activemq" | "artemis" => Self::ActiveMq,
            "nats" => Self::Nats,
            "mqtt" => Self::Mqtt,
            "mqtt-mica" | "mica-mqtt" | "mica" => Self::MqttMica,
            "ons" => Self::Ons,
            "tdmq" => Self::Tdmq,
            "sqs" => Self::Sqs,
            _ => Self::None,
        }
    }

    /// Returns a stable configuration value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Disruptor => "disruptor",
            Self::Rabbit => "rabbit",
            Self::Kafka => "kafka",
            Self::Rocket => "rocket",
            Self::Pulsar => "pulsar",
            Self::RedisStream => "redis-stream",
            Self::ActiveMq => "activemq",
            Self::Nats => "nats",
            Self::Mqtt => "mqtt",
            Self::MqttMica => "mqtt-mica",
            Self::Ons => "ons",
            Self::Tdmq => "tdmq",
            Self::Sqs => "sqs",
        }
    }
}

/// Ordered-delivery partition strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionKeyStrategy {
    /// Do not set a key.
    None,
    /// Order by tag.
    Tag,
    /// Order by tenant.
    Tenant,
    /// Order by tag and tenant.
    #[default]
    TagTenant,
    /// Adapter or application supplies the key.
    Custom,
}

/// Shared MQ configuration. Credentials remain adapter-specific secret types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MqProperties {
    /// Whether this client is enabled.
    pub enabled: bool,
    /// Selected broker.
    pub broker: BrokerType,
    /// Server endpoint without embedded credentials.
    pub server: String,
    /// Environment namespace.
    pub namespace: String,
    /// Persist deliveries before handler invocation.
    pub persist: bool,
    /// Whether the broker owns acknowledgments.
    pub auto_ack: bool,
    /// Handler retry count before dead-letter disposition.
    pub retries: u32,
    /// Default topic.
    pub default_topic: String,
    /// Ordered-delivery strategy.
    pub partition_key_strategy: PartitionKeyStrategy,
}

impl Default for MqProperties {
    fn default() -> Self {
        Self {
            enabled: false,
            broker: BrokerType::None,
            server: String::new(),
            namespace: String::new(),
            persist: false,
            auto_ack: false,
            retries: 0,
            default_topic: "DEFAULT".to_owned(),
            partition_key_strategy: PartitionKeyStrategy::TagTenant,
        }
    }
}

/// Physical destination after namespace/topic/tag resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TopicAddress {
    /// Namespace or environment prefix.
    pub namespace: Option<String>,
    /// Logical topic.
    pub topic: String,
    /// Optional message tag.
    pub tag: Option<String>,
    /// Broker-specific separator.
    pub separator: String,
}

impl TopicAddress {
    /// Builds a validated destination.
    pub fn new(
        namespace: Option<String>,
        topic: impl Into<String>,
        tag: Option<String>,
        separator: impl Into<String>,
    ) -> MqResult<Self> {
        let topic = topic.into();
        if topic.trim().is_empty() {
            return Err(MqError::InvalidInput {
                message: "message topic must not be blank".to_owned(),
            });
        }
        let separator = separator.into();
        Ok(Self {
            namespace: namespace.filter(|value| !value.is_empty()),
            topic,
            tag: tag.filter(|value| !value.is_empty()),
            separator: if separator.is_empty() {
                ".".to_owned()
            } else {
                separator
            },
        })
    }

    /// Resolves `namespace<sep>topic<sep>tag`.
    pub fn physical_name(&self) -> String {
        [
            self.namespace.as_deref(),
            Some(self.topic.as_str()),
            self.tag.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(&self.separator)
    }
}

/// Broker-neutral immutable message envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MqEvent {
    /// Stable `UUIDv7` message identifier.
    pub message_id: Uuid,
    /// Event kind used by handler support filters.
    pub event_type: String,
    /// Schema version.
    pub event_version: u32,
    /// Destination.
    pub destination: TopicAddress,
    /// Explicit target broker, if any.
    pub broker: Option<BrokerType>,
    /// Tenant context.
    pub tenant_id: Option<String>,
    /// Correlation identifier.
    pub correlation_id: Option<String>,
    /// Causation identifier.
    pub causation_id: Option<String>,
    /// UTC creation time.
    pub occurred_at: OffsetDateTime,
    /// Delivery attempt, starting at one.
    pub delivery_attempt: u32,
    /// Application payload.
    pub payload: Vec<u8>,
    /// Portable string headers.
    pub headers: BTreeMap<String, String>,
}

impl MqEvent {
    /// Creates a version-one event with `UUIDv7` identity.
    pub fn new(
        event_type: impl Into<String>,
        destination: TopicAddress,
        payload: Vec<u8>,
    ) -> MqResult<Self> {
        let event_type = event_type.into();
        if event_type.trim().is_empty() {
            return Err(MqError::InvalidInput {
                message: "event_type must not be blank".to_owned(),
            });
        }
        Ok(Self {
            message_id: Uuid::now_v7(),
            event_type,
            event_version: 1,
            destination,
            broker: None,
            tenant_id: None,
            correlation_id: None,
            causation_id: None,
            occurred_at: OffsetDateTime::now_utc(),
            delivery_attempt: 1,
            payload,
            headers: BTreeMap::new(),
        })
    }

    /// Computes the default ordered-delivery key.
    pub fn partition_key(&self, strategy: PartitionKeyStrategy) -> Option<String> {
        match strategy {
            PartitionKeyStrategy::None | PartitionKeyStrategy::Custom => None,
            PartitionKeyStrategy::Tag => self.destination.tag.clone(),
            PartitionKeyStrategy::Tenant => self.tenant_id.clone(),
            PartitionKeyStrategy::TagTenant => {
                match (self.destination.tag.as_deref(), self.tenant_id.as_deref()) {
                    (Some(tag), Some(tenant)) => Some(format!("{tag}|{tenant}")),
                    (Some(tag), None) => Some(tag.to_owned()),
                    (None, Some(tenant)) => Some(tenant.to_owned()),
                    (None, None) => None,
                }
            }
        }
    }
}

/// A broker delivery with its acknowledgment port.
#[derive(Clone)]
pub struct Delivery {
    /// Message envelope.
    pub event: MqEvent,
    /// Broker acknowledgment.
    pub acknowledgment: Arc<dyn Acknowledgment>,
}

impl std::fmt::Debug for Delivery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Delivery")
            .field("event", &self.event)
            .field("ack_state", &self.acknowledgment.state())
            .finish()
    }
}

/// Handler-directed settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerOutcome {
    /// Mark successful.
    Ack,
    /// Retry through broker redelivery.
    Requeue,
    /// Do not retry; dead-letter or discard.
    Discard,
}

/// Broker publish result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    /// Stable message identifier.
    pub message_id: Uuid,
    /// Broker-native identifier when available.
    pub broker_message_id: Option<String>,
    /// Broker family.
    pub broker: BrokerType,
    /// Resolved physical destination.
    pub destination: String,
}
