//! `SeaORM` repository adapter with an executable `SQLite` vertical slice.

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
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, DbBackend,
    DbErr, Statement, TransactionTrait,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

const ADAPTER: &str = "seaorm-sqlite";

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-data-jpa",
    rust_package: "ddd4r-data-seaorm",
    group: "data",
    maturity: ModuleMaturity::InProgress,
};

/// `SeaORM` backend capability declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct SeaOrmBackend;

impl DataBackend for SeaOrmBackend {
    fn name(&self) -> &'static str {
        "seaorm"
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

/// Generic aggregate repository stored as canonical JSON through `SeaORM`.
#[derive(Debug, Clone)]
pub struct SeaOrmRepository<A> {
    database: DatabaseConnection,
    aggregate_type: &'static str,
    marker: PhantomData<fn() -> A>,
}

impl<A> SeaOrmRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    /// Creates the adapter and initializes its aggregate table.
    pub async fn new(database: DatabaseConnection) -> DddResult<Self> {
        database
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS ddd4r_aggregate (\
                 aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL, \
                 version INTEGER NOT NULL, payload TEXT NOT NULL, \
                 PRIMARY KEY (aggregate_type, aggregate_id))",
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        database
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS ddd4r_outbox (\
                 event_id TEXT PRIMARY KEY, aggregate_type TEXT NOT NULL, \
                 aggregate_id TEXT NOT NULL, aggregate_version INTEGER NOT NULL, \
                 event_type TEXT NOT NULL, envelope TEXT NOT NULL, status TEXT NOT NULL)",
            )
            .await
            .map_err(|error| adapter_error(&error))?;
        Ok(Self {
            database,
            aggregate_type: type_name::<A>(),
            marker: PhantomData,
        })
    }

    /// Creates a deterministic single-connection in-memory `SQLite` repository.
    pub async fn connect_memory() -> DddResult<Self> {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).min_connections(1);
        let database = Database::connect(options)
            .await
            .map_err(|error| adapter_error(&error))?;
        Self::new(database).await
    }

    /// Starts a caller-managed `SeaORM` transaction.
    pub async fn begin(&self) -> DddResult<DatabaseTransaction> {
        self.database
            .begin()
            .await
            .map_err(|error| adapter_error(&error))
    }

    /// Returns the configured database connection.
    pub const fn database(&self) -> &DatabaseConnection {
        &self.database
    }

    async fn persisted_version(&self, id: &A::Id) -> DddResult<Option<u64>> {
        let row = self
            .database
            .query_one_raw(statement(
                "SELECT version FROM ddd4r_aggregate \
                 WHERE aggregate_type = ? AND aggregate_id = ?",
                [self.aggregate_type.into(), id.to_string().into()],
            ))
            .await
            .map_err(|error| adapter_error(&error))?;
        row.map(|row| {
            row.try_get_by::<i64, _>("version")
                .map_err(|error| adapter_error(&error))
                .and_then(version_from_i64)
        })
        .transpose()
    }

    async fn all_values(&self) -> DddResult<Vec<A>> {
        self.database
            .query_all_raw(statement(
                "SELECT payload FROM ddd4r_aggregate WHERE aggregate_type = ?",
                [self.aggregate_type.into()],
            ))
            .await
            .map_err(|error| adapter_error(&error))?
            .into_iter()
            .map(|row| {
                let payload = row
                    .try_get_by::<String, _>("payload")
                    .map_err(|error| adapter_error(&error))?;
                serde_json::from_str(&payload).map_err(Into::into)
            })
            .collect()
    }

    async fn matching(&self, query: &Query<A>) -> DddResult<Vec<A>> {
        matching_aggregates(self.all_values().await?, query)
    }

    async fn append_outbox(
        tx: &DatabaseTransaction,
        events: &[ddd4r_core::event::EventEnvelope],
    ) -> DddResult<()> {
        for event in events {
            tx.execute_raw(statement(
                "INSERT INTO ddd4r_outbox \
                 (event_id, aggregate_type, aggregate_id, aggregate_version, \
                  event_type, envelope, status) VALUES (?, ?, ?, ?, ?, ?, 'pending')",
                [
                    event.event_id.to_string().into(),
                    event.aggregate_type.clone().into(),
                    event.aggregate_id.clone().into(),
                    version_to_i64(event.aggregate_version)?.into(),
                    event.event_type.clone().into(),
                    serde_json::to_string(event)?.into(),
                ],
            ))
            .await
            .map_err(|error| adapter_error(&error))?;
        }
        Ok(())
    }
}

impl<A> TransactionalEventRepository<A> for SeaOrmRepository<A>
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
                let actual = tx
                    .query_one_raw(statement(
                        "SELECT version FROM ddd4r_aggregate \
                         WHERE aggregate_type = ? AND aggregate_id = ?",
                        [
                            self.aggregate_type.into(),
                            aggregate.id().to_string().into(),
                        ],
                    ))
                    .await
                    .map_err(|error| adapter_error(&error))?
                    .map(|row| {
                        row.try_get_by::<i64, _>("version")
                            .map_err(|error| adapter_error(&error))
                            .and_then(version_from_i64)
                    })
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
                    tx.execute_raw(statement(
                        "INSERT INTO ddd4r_aggregate \
                         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                        [
                            self.aggregate_type.into(),
                            aggregate.id().to_string().into(),
                            version_to_i64(next)?.into(),
                            payload.into(),
                        ],
                    ))
                    .await
                } else {
                    tx.execute_raw(statement(
                        "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                         WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                        [
                            version_to_i64(next)?.into(),
                            payload.into(),
                            self.aggregate_type.into(),
                            aggregate.id().to_string().into(),
                            version_to_i64(expected)?.into(),
                        ],
                    ))
                    .await
                }
                .map_err(|error| adapter_error(&error))?
                .rows_affected();
                if affected != 1 {
                    return Err(DddError::OptimisticLockConflict {
                        aggregate: type_name::<A>(),
                        expected,
                        actual: actual.unwrap_or(0),
                    });
                }
                Self::append_outbox(&tx, &events).await
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
            self.database
                .query_all_raw(statement(
                    "SELECT envelope FROM ddd4r_outbox \
                     WHERE status = 'pending' ORDER BY rowid",
                    [],
                ))
                .await
                .map_err(|error| adapter_error(&error))?
                .into_iter()
                .map(|row| {
                    let envelope = row
                        .try_get_by::<String, _>("envelope")
                        .map_err(|error| adapter_error(&error))?;
                    serde_json::from_str(&envelope).map_err(Into::into)
                })
                .collect()
        })
    }
}

impl<A> Repository<A> for SeaOrmRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    fn find_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<Option<A>>> {
        Box::pin(async move {
            let row = self
                .database
                .query_one_raw(statement(
                    "SELECT payload FROM ddd4r_aggregate \
                     WHERE aggregate_type = ? AND aggregate_id = ?",
                    [self.aggregate_type.into(), id.to_string().into()],
                ))
                .await
                .map_err(|error| adapter_error(&error))?;
            row.map(|row| {
                let payload = row
                    .try_get_by::<String, _>("payload")
                    .map_err(|error| adapter_error(&error))?;
                serde_json::from_str(&payload).map_err(Into::into)
            })
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
                self.database
                    .execute_raw(statement(
                        "INSERT INTO ddd4r_aggregate \
                         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                        [
                            self.aggregate_type.into(),
                            aggregate.id().to_string().into(),
                            version_to_i64(next)?.into(),
                            payload.into(),
                        ],
                    ))
                    .await
            } else {
                self.database
                    .execute_raw(statement(
                        "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                         WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                        [
                            version_to_i64(next)?.into(),
                            payload.into(),
                            self.aggregate_type.into(),
                            aggregate.id().to_string().into(),
                            version_to_i64(expected)?.into(),
                        ],
                    ))
                    .await
            };
            match result {
                Ok(result) if result.rows_affected() == 1 => Ok(()),
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
            self.database
                .execute_raw(statement(
                    "DELETE FROM ddd4r_aggregate WHERE aggregate_type = ? AND aggregate_id = ?",
                    [self.aggregate_type.into(), id.to_string().into()],
                ))
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

fn statement<const N: usize>(sql: &str, values: [sea_orm::Value; N]) -> Statement {
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}

fn version_to_i64(version: u64) -> DddResult<i64> {
    i64::try_from(version).map_err(|error| DddError::Adapter {
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

fn adapter_error(error: &DbErr) -> DddError {
    DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    }
}
