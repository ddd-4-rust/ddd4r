//! `RBatis` repository adapter backed by the official 4.9 executor API.

#![forbid(unsafe_code)]

use std::any::type_name;
use std::fmt::Display;
use std::marker::PhantomData;

use ddd4r_core::domain::AggregateRoot;
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use ddd4r_core::query::{Page, Query};
use ddd4r_core::repository::{Repository, RepositoryRow};
use ddd4r_core::{DddError, DddResult};
use ddd4r_data::{
    BackendCapabilities, Capability, DataBackend, DatabaseKind, TransactionalEventRepository,
    matching_aggregates, page_aggregates, selected_rows,
};
use futures::future::BoxFuture;
use rbatis::RBatis;
use rbatis::executor::RBatisTxExecutor;
use rbdc_sqlite::SqliteDriver;
use rbs::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;

const ADAPTER: &str = "rbatis-sqlite";

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-data-mybatis",
    rust_package: "ddd4r-data-rbatis",
    group: "data",
    maturity: ModuleMaturity::InProgress,
};

/// `RBatis` backend capability declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct RbatisBackend;

impl DataBackend for RbatisBackend {
    fn name(&self) -> &'static str {
        "rbatis"
    }

    fn databases(&self) -> &'static [DatabaseKind] {
        &[DatabaseKind::Sqlite]
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            crud: Capability::Supported,
            batch: Capability::Supported,
            query: Capability::Supported,
            optimistic_lock: Capability::Supported,
            logical_delete: Capability::Planned,
            unit_of_work: Capability::Supported,
            transactional_outbox: Capability::Supported,
            tenant_isolation: Capability::Planned,
            data_scope: Capability::Planned,
            audit_fill: Capability::Planned,
            event_sourcing: Capability::Planned,
        }
    }
}

/// Generic aggregate repository stored as JSON through `RBatis`.
#[derive(Debug, Clone)]
pub struct RbatisRepository<A> {
    rbatis: RBatis,
    aggregate_type: &'static str,
    marker: PhantomData<fn() -> A>,
}

impl<A> RbatisRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    /// Creates the adapter and initializes its aggregate table.
    pub async fn new(rbatis: RBatis) -> DddResult<Self> {
        rbatis
            .exec(
                "CREATE TABLE IF NOT EXISTS ddd4r_aggregate (\
                 aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL, \
                 version INTEGER NOT NULL, payload TEXT NOT NULL, \
                 PRIMARY KEY (aggregate_type, aggregate_id))",
                Vec::new(),
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        rbatis
            .exec(
                "CREATE TABLE IF NOT EXISTS ddd4r_outbox (\
                 event_id TEXT PRIMARY KEY, aggregate_type TEXT NOT NULL, \
                 aggregate_id TEXT NOT NULL, aggregate_version INTEGER NOT NULL, \
                 event_type TEXT NOT NULL, envelope TEXT NOT NULL, status TEXT NOT NULL)",
                Vec::new(),
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        Ok(Self {
            rbatis,
            aggregate_type: type_name::<A>(),
            marker: PhantomData,
        })
    }

    /// Creates an in-memory `SQLite` repository through the official driver.
    pub async fn connect_memory() -> DddResult<Self> {
        let rbatis = RBatis::new();
        rbatis
            .link(SqliteDriver {}, "sqlite://:memory:")
            .await
            .map_err(|error| adapter_error(&error))?;
        Self::new(rbatis).await
    }

    /// Starts an explicit `RBatis` transaction.
    pub async fn begin(&self) -> DddResult<RBatisTxExecutor> {
        self.rbatis
            .acquire_begin()
            .await
            .map_err(|error| adapter_error(&error))
    }

    /// Returns the configured `RBatis` executor.
    pub const fn rbatis(&self) -> &RBatis {
        &self.rbatis
    }

    async fn persisted_version(&self, id: &A::Id) -> DddResult<Option<u64>> {
        let raw = self
            .rbatis
            .query(
                "SELECT version FROM ddd4r_aggregate \
                 WHERE aggregate_type = ? AND aggregate_id = ?",
                vec![
                    Value::String(self.aggregate_type.to_owned()),
                    Value::String(id.to_string()),
                ],
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        rows(&raw)?
            .first()
            .and_then(|row| row["version"].as_i64())
            .map(version_from_i64)
            .transpose()
    }

    async fn all_values(&self) -> DddResult<Vec<A>> {
        let raw = self
            .rbatis
            .query(
                "SELECT payload FROM ddd4r_aggregate WHERE aggregate_type = ?",
                vec![Value::String(self.aggregate_type.to_owned())],
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        rows(&raw)?
            .iter()
            .map(|row| decode_payload(&row["payload"]))
            .collect()
    }

    async fn matching(&self, query: &Query<A>) -> DddResult<Vec<A>> {
        matching_aggregates(self.all_values().await?, query)
    }
}

impl<A> TransactionalEventRepository<A> for RbatisRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    fn save_with_outbox<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let expected = aggregate.version();
            let events = aggregate.recorded_events().to_vec();
            let tx = self.begin().await?;
            let result: DddResult<()> = async {
                let raw = tx
                    .query(
                        "SELECT version FROM ddd4r_aggregate \
                         WHERE aggregate_type = ? AND aggregate_id = ?",
                        vec![
                            Value::String(self.aggregate_type.to_owned()),
                            Value::String(aggregate.id().to_string()),
                        ],
                    )
                    .await
                    .map_err(|error| adapter_error(&error))?;
                let actual = rows(&raw)?
                    .first()
                    .and_then(|row| row["version"].as_i64())
                    .map(version_from_i64)
                    .transpose()?;
                if actual.is_some_and(|actual| actual != expected)
                    || (actual.is_none() && expected != 0)
                {
                    return Err(DddError::OptimisticLockConflict {
                        aggregate: type_name::<A>(),
                        expected,
                        actual: actual.unwrap_or(0),
                    });
                }
                let next = expected.saturating_add(1);
                aggregate.set_version(next);
                let payload = serde_json::to_string(aggregate)?;
                let affected = if actual.is_none() {
                    tx.exec(
                        "INSERT INTO ddd4r_aggregate \
                         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                        vec![
                            Value::String(self.aggregate_type.to_owned()),
                            Value::String(aggregate.id().to_string()),
                            Value::I64(version_to_i64(next)?),
                            Value::String(payload),
                        ],
                    )
                    .await
                } else {
                    tx.exec(
                        "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                         WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                        vec![
                            Value::I64(version_to_i64(next)?),
                            Value::String(payload),
                            Value::String(self.aggregate_type.to_owned()),
                            Value::String(aggregate.id().to_string()),
                            Value::I64(version_to_i64(expected)?),
                        ],
                    )
                    .await
                }
                .map_err(|error| adapter_error(&error))?
                .rows_affected;
                if affected != 1 {
                    return Err(DddError::OptimisticLockConflict {
                        aggregate: type_name::<A>(),
                        expected,
                        actual: actual.unwrap_or(0),
                    });
                }
                for event in &events {
                    tx.exec(
                        "INSERT INTO ddd4r_outbox \
                         (event_id, aggregate_type, aggregate_id, aggregate_version, \
                          event_type, envelope, status) VALUES (?, ?, ?, ?, ?, ?, 'pending')",
                        vec![
                            Value::String(event.event_id.to_string()),
                            Value::String(event.aggregate_type.clone()),
                            Value::String(event.aggregate_id.clone()),
                            Value::I64(version_to_i64(event.aggregate_version)?),
                            Value::String(event.event_type.clone()),
                            Value::String(serde_json::to_string(event)?),
                        ],
                    )
                    .await
                    .map_err(|error| adapter_error(&error))?;
                }
                Ok(())
            }
            .await;

            if let Err(error) = result {
                let _ = tx.rollback().await;
                aggregate.set_version(expected);
                return Err(error);
            }
            if let Err(error) = tx.commit().await {
                aggregate.set_version(expected);
                return Err(adapter_error(&error));
            }
            aggregate.clear_events();
            Ok(())
        })
    }

    fn pending_outbox_events(
        &self,
    ) -> BoxFuture<'_, DddResult<Vec<ddd4r_core::event::EventEnvelope>>> {
        Box::pin(async move {
            let raw = self
                .rbatis
                .query(
                    "SELECT envelope FROM ddd4r_outbox \
                     WHERE status = 'pending' ORDER BY rowid",
                    Vec::new(),
                )
                .await
                .map_err(|error| adapter_error(&error))?;
            rows(&raw)?
                .iter()
                .map(|row| decode_payload(&row["envelope"]))
                .collect()
        })
    }
}

impl<A> Repository<A> for RbatisRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    fn find_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<Option<A>>> {
        Box::pin(async move {
            let raw = self
                .rbatis
                .query(
                    "SELECT payload FROM ddd4r_aggregate \
                     WHERE aggregate_type = ? AND aggregate_id = ?",
                    vec![
                        Value::String(self.aggregate_type.to_owned()),
                        Value::String(id.to_string()),
                    ],
                )
                .await
                .map_err(|error| adapter_error(&error))?;
            rows(&raw)?
                .first()
                .map(|row| decode_payload(&row["payload"]))
                .transpose()
        })
    }

    fn save<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let expected = aggregate.version();
            let actual = self.persisted_version(aggregate.id()).await?;
            if actual.is_some_and(|actual| actual != expected)
                || (actual.is_none() && expected != 0)
            {
                return Err(DddError::OptimisticLockConflict {
                    aggregate: type_name::<A>(),
                    expected,
                    actual: actual.unwrap_or(0),
                });
            }
            let next = expected.saturating_add(1);
            aggregate.set_version(next);
            let payload = match serde_json::to_string(aggregate) {
                Ok(payload) => payload,
                Err(error) => {
                    aggregate.set_version(expected);
                    return Err(error.into());
                }
            };
            let result = if actual.is_none() {
                self.rbatis
                    .exec(
                        "INSERT INTO ddd4r_aggregate \
                         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                        vec![
                            Value::String(self.aggregate_type.to_owned()),
                            Value::String(aggregate.id().to_string()),
                            Value::I64(version_to_i64(next)?),
                            Value::String(payload),
                        ],
                    )
                    .await
            } else {
                self.rbatis
                    .exec(
                        "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                         WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                        vec![
                            Value::I64(version_to_i64(next)?),
                            Value::String(payload),
                            Value::String(self.aggregate_type.to_owned()),
                            Value::String(aggregate.id().to_string()),
                            Value::I64(version_to_i64(expected)?),
                        ],
                    )
                    .await
            };
            match result {
                Ok(result) if result.rows_affected == 1 => Ok(()),
                Ok(_) => {
                    aggregate.set_version(expected);
                    Err(DddError::OptimisticLockConflict {
                        aggregate: type_name::<A>(),
                        expected,
                        actual: self.persisted_version(aggregate.id()).await?.unwrap_or(0),
                    })
                }
                Err(error) => {
                    aggregate.set_version(expected);
                    Err(adapter_error(&error))
                }
            }
        })
    }

    fn delete_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            self.rbatis
                .exec(
                    "DELETE FROM ddd4r_aggregate WHERE aggregate_type = ? AND aggregate_id = ?",
                    vec![
                        Value::String(self.aggregate_type.to_owned()),
                        Value::String(id.to_string()),
                    ],
                )
                .await
                .map_err(|error| adapter_error(&error))?;
            Ok(())
        })
    }

    fn find_all(&self) -> BoxFuture<'_, DddResult<Vec<A>>> {
        Box::pin(self.all_values())
    }

    fn find_list<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<Vec<A>>> {
        Box::pin(async move { Ok(page_aggregates(self.matching(query).await?, query)?.records) })
    }

    fn page<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<Page<A>>> {
        Box::pin(async move { page_aggregates(self.matching(query).await?, query) })
    }

    fn maps<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<Vec<RepositoryRow>>> {
        Box::pin(async move { selected_rows(self.find_list(query).await?, query) })
    }

    fn delete_by_query<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move {
            let matches = self.matching(query).await?;
            for aggregate in &matches {
                self.delete_by_id(aggregate.id()).await?;
            }
            Ok(!matches.is_empty())
        })
    }
}

fn version_to_i64(version: u64) -> DddResult<i64> {
    i64::try_from(version).map_err(|error| DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    })
}

fn rows(value: &Value) -> DddResult<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| DddError::Adapter {
            adapter: ADAPTER,
            message: "RBatis query result must be an array".to_owned(),
        })
}

fn decode_payload<A>(value: &Value) -> DddResult<A>
where
    A: DeserializeOwned,
{
    if let Some(payload) = value.as_str() {
        return serde_json::from_str(payload).map_err(Into::into);
    }
    rbs::from_value_ref(value).map_err(|error| DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    })
}

fn version_from_i64(version: i64) -> DddResult<u64> {
    u64::try_from(version).map_err(|error| DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    })
}

fn adapter_error(error: &rbatis::Error) -> DddError {
    DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    }
}
