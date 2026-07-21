//! `SQLx` repository adapter with an executable `SQLite` vertical slice.

#![forbid(unsafe_code)]

use std::any::type_name;
use std::fmt::Display;
use std::marker::PhantomData;

use ddd4r_core::domain::AggregateRoot;
use ddd4r_core::query::{Page, Query};
use ddd4r_core::repository::{Repository, RepositoryRow};
use ddd4r_core::{DddError, DddResult};
use ddd4r_data::{
    BackendCapabilities, Capability, DataBackend, DatabaseKind, TransactionalEventRepository,
    matching_aggregates, page_aggregates, selected_rows,
};
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, SqlitePool, Transaction};

const ADAPTER: &str = "sqlx-sqlite";

/// `SQLx` backend capability declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlxBackend;

impl DataBackend for SqlxBackend {
    fn name(&self) -> &'static str {
        "sqlx"
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

/// Generic aggregate repository stored as canonical JSON with a relational version column.
#[derive(Debug, Clone)]
pub struct SqlxRepository<A> {
    pool: SqlitePool,
    aggregate_type: &'static str,
    marker: PhantomData<fn() -> A>,
}

impl<A> SqlxRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    /// Creates the adapter and initializes its aggregate table.
    pub async fn new(pool: SqlitePool) -> DddResult<Self> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ddd4r_aggregate (\
             aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL, \
             version INTEGER NOT NULL, payload TEXT NOT NULL, \
             PRIMARY KEY (aggregate_type, aggregate_id))",
        )
        .execute(&pool)
        .await
        .map_err(|error| adapter_error(&error))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ddd4r_outbox (\
             event_id TEXT PRIMARY KEY, aggregate_type TEXT NOT NULL, \
             aggregate_id TEXT NOT NULL, aggregate_version INTEGER NOT NULL, \
             event_type TEXT NOT NULL, envelope TEXT NOT NULL, status TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|error| adapter_error(&error))?;
        Ok(Self {
            pool,
            aggregate_type: type_name::<A>(),
            marker: PhantomData,
        })
    }

    /// Creates a deterministic single-connection in-memory `SQLite` adapter.
    pub async fn connect_memory() -> DddResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(|error| adapter_error(&error))?;
        Self::new(pool).await
    }

    /// Starts a caller-managed `SQLx` transaction for aggregate/outbox composition.
    pub async fn begin(&self) -> DddResult<Transaction<'static, Sqlite>> {
        self.pool
            .begin()
            .await
            .map_err(|error| adapter_error(&error))
    }

    /// Returns the underlying pool for infrastructure composition.
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn persisted_version(&self, id: &A::Id) -> DddResult<Option<u64>> {
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM ddd4r_aggregate \
             WHERE aggregate_type = ? AND aggregate_id = ?",
        )
        .bind(self.aggregate_type)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| adapter_error(&error))?;
        version.map(version_from_i64).transpose()
    }

    async fn all_values(&self) -> DddResult<Vec<A>> {
        let payloads = sqlx::query_scalar::<_, String>(
            "SELECT payload FROM ddd4r_aggregate WHERE aggregate_type = ?",
        )
        .bind(self.aggregate_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| adapter_error(&error))?;
        payloads
            .into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .collect()
    }

    async fn matching(&self, query: &Query<A>) -> DddResult<Vec<A>> {
        matching_aggregates(self.all_values().await?, query)
    }
}

impl<A> TransactionalEventRepository<A> for SqlxRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    fn save_with_outbox<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let expected = aggregate.version();
            let events = aggregate.recorded_events().to_vec();
            let mut tx = self.begin().await?;
            let result: DddResult<()> = async {
                let actual = sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM ddd4r_aggregate \
                     WHERE aggregate_type = ? AND aggregate_id = ?",
                )
                .bind(self.aggregate_type)
                .bind(aggregate.id().to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| adapter_error(&error))?
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
                    sqlx::query(
                        "INSERT INTO ddd4r_aggregate \
                         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                    )
                    .bind(self.aggregate_type)
                    .bind(aggregate.id().to_string())
                    .bind(version_to_i64(next)?)
                    .bind(payload)
                    .execute(&mut *tx)
                    .await
                } else {
                    sqlx::query(
                        "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                         WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                    )
                    .bind(version_to_i64(next)?)
                    .bind(payload)
                    .bind(self.aggregate_type)
                    .bind(aggregate.id().to_string())
                    .bind(version_to_i64(expected)?)
                    .execute(&mut *tx)
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
                for event in &events {
                    sqlx::query(
                        "INSERT INTO ddd4r_outbox \
                         (event_id, aggregate_type, aggregate_id, aggregate_version, \
                          event_type, envelope, status) VALUES (?, ?, ?, ?, ?, ?, 'pending')",
                    )
                    .bind(event.event_id.to_string())
                    .bind(&event.aggregate_type)
                    .bind(&event.aggregate_id)
                    .bind(version_to_i64(event.aggregate_version)?)
                    .bind(&event.event_type)
                    .bind(serde_json::to_string(event)?)
                    .execute(&mut *tx)
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
            let envelopes = sqlx::query_scalar::<_, String>(
                "SELECT envelope FROM ddd4r_outbox WHERE status = 'pending' ORDER BY rowid",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|error| adapter_error(&error))?;
            envelopes
                .into_iter()
                .map(|envelope| serde_json::from_str(&envelope).map_err(Into::into))
                .collect()
        })
    }
}

impl<A> Repository<A> for SqlxRepository<A>
where
    A: AggregateRoot + Serialize + DeserializeOwned,
    A::Id: Display,
{
    fn find_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<Option<A>>> {
        Box::pin(async move {
            let payload = sqlx::query_scalar::<_, String>(
                "SELECT payload FROM ddd4r_aggregate \
                 WHERE aggregate_type = ? AND aggregate_id = ?",
            )
            .bind(self.aggregate_type)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| adapter_error(&error))?;
            payload
                .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
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
                sqlx::query(
                    "INSERT INTO ddd4r_aggregate \
                     (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
                )
                .bind(self.aggregate_type)
                .bind(aggregate.id().to_string())
                .bind(version_to_i64(next)?)
                .bind(payload)
                .execute(&self.pool)
                .await
            } else {
                sqlx::query(
                    "UPDATE ddd4r_aggregate SET version = ?, payload = ? \
                     WHERE aggregate_type = ? AND aggregate_id = ? AND version = ?",
                )
                .bind(version_to_i64(next)?)
                .bind(payload)
                .bind(self.aggregate_type)
                .bind(aggregate.id().to_string())
                .bind(version_to_i64(expected)?)
                .execute(&self.pool)
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
            sqlx::query(
                "DELETE FROM ddd4r_aggregate WHERE aggregate_type = ? AND aggregate_id = ?",
            )
            .bind(self.aggregate_type)
            .bind(id.to_string())
            .execute(&self.pool)
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

fn version_from_i64(version: i64) -> DddResult<u64> {
    u64::try_from(version).map_err(|error| DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    })
}

fn adapter_error(error: &sqlx::Error) -> DddError {
    DddError::Adapter {
        adapter: ADAPTER,
        message: error.to_string(),
    }
}
