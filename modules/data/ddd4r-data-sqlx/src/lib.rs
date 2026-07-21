//! `SQLx` repository adapter with an executable `SQLite` vertical slice.

#![forbid(unsafe_code)]

use std::any::type_name;
use std::cmp::Ordering;
use std::fmt::Display;
use std::marker::PhantomData;

use ddd4r_core::domain::AggregateRoot;
use ddd4r_core::query::{Condition, Operator, Page, Query};
use ddd4r_core::repository::{Repository, RepositoryRow};
use ddd4r_core::{DddError, DddResult};
use ddd4r_data::{BackendCapabilities, Capability, DataBackend, DatabaseKind};
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
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
            transactional_outbox: Capability::Planned,
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
        let mut values = Vec::new();
        for aggregate in self.all_values().await? {
            if matches_query(&aggregate, query)? {
                values.push(aggregate);
            }
        }
        values.sort_by(|left, right| compare_aggregates(left, right, query));
        Ok(values)
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
        Box::pin(async move {
            let values = self.matching(query).await?;
            Ok(page_slice(values, query).0)
        })
    }

    fn page<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<Page<A>>> {
        Box::pin(async move {
            let values = self.matching(query).await?;
            let total = u64::try_from(values.len()).map_err(|error| DddError::Adapter {
                adapter: ADAPTER,
                message: error.to_string(),
            })?;
            let (records, size) = page_slice(values, query);
            Ok(Page {
                records,
                total,
                current: query.page.current,
                size,
            })
        })
    }

    fn maps<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<Vec<RepositoryRow>>> {
        Box::pin(async move {
            self.find_list(query)
                .await?
                .into_iter()
                .map(|aggregate| {
                    let Value::Object(mut row) = serde_json::to_value(aggregate)? else {
                        return Err(DddError::Adapter {
                            adapter: ADAPTER,
                            message: "aggregate must serialize as an object".to_owned(),
                        });
                    };
                    if !query.select_columns.is_empty() {
                        row.retain(|key, _| query.select_columns.contains(key));
                    }
                    Ok(row)
                })
                .collect()
        })
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

fn matches_query<A>(aggregate: &A, query: &Query<A>) -> DddResult<bool>
where
    A: Serialize,
{
    let Value::Object(object) = serde_json::to_value(aggregate)? else {
        return Ok(false);
    };
    Ok(query
        .conditions
        .iter()
        .all(|condition| matches_condition(object.get(&condition.property), condition)))
}

fn matches_condition(value: Option<&Value>, condition: &Condition) -> bool {
    let first = condition.operands.first();
    match condition.operator {
        Operator::Eq => value == first,
        Operator::Ne => value != first,
        Operator::Gt => compare_values(value, first) == Ordering::Greater,
        Operator::Ge => matches!(
            compare_values(value, first),
            Ordering::Greater | Ordering::Equal
        ),
        Operator::Lt => compare_values(value, first) == Ordering::Less,
        Operator::Le => matches!(
            compare_values(value, first),
            Ordering::Less | Ordering::Equal
        ),
        Operator::Like => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.contains(operand))),
        Operator::LikeLeft => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.ends_with(operand))),
        Operator::LikeRight => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.starts_with(operand))),
        Operator::NotLike => !matches_condition(
            value,
            &Condition {
                property: condition.property.clone(),
                operator: Operator::Like,
                operands: condition.operands.clone(),
            },
        ),
        Operator::In => value.is_some_and(|value| condition.operands.contains(value)),
        Operator::NotIn => value.is_none_or(|value| !condition.operands.contains(value)),
        Operator::IsNull => value.is_none_or(Value::is_null),
        Operator::IsNotNull => value.is_some_and(|value| !value.is_null()),
    }
}

fn compare_aggregates<A>(left: &A, right: &A, query: &Query<A>) -> Ordering
where
    A: Serialize,
{
    let left = serde_json::to_value(left).unwrap_or(Value::Null);
    let right = serde_json::to_value(right).unwrap_or(Value::Null);
    for order in &query.orders {
        let ordering = compare_values(left.get(&order.property), right.get(&order.property));
        if ordering != Ordering::Equal {
            return if order.ascending {
                ordering
            } else {
                ordering.reverse()
            };
        }
    }
    Ordering::Equal
}

fn compare_values(left: Option<&Value>, right: Option<&Value>) -> Ordering {
    match (left, right) {
        (Some(Value::Number(left)), Some(Value::Number(right))) => left
            .as_f64()
            .partial_cmp(&right.as_f64())
            .unwrap_or(Ordering::Equal),
        (Some(Value::String(left)), Some(Value::String(right))) => left.cmp(right),
        (Some(Value::Bool(left)), Some(Value::Bool(right))) => left.cmp(right),
        (None | Some(Value::Null), None | Some(Value::Null)) => Ordering::Equal,
        (None | Some(Value::Null), _) => Ordering::Less,
        (_, None | Some(Value::Null)) => Ordering::Greater,
        (Some(left), Some(right)) => left.to_string().cmp(&right.to_string()),
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn page_slice<A>(values: Vec<A>, query: &Query<A>) -> (Vec<A>, u64) {
    let Some(size) = query.page.size else {
        let size = u64::try_from(values.len()).unwrap_or(u64::MAX);
        return (values, size);
    };
    let offset = query.page.current.saturating_sub(1).saturating_mul(size);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let limit = usize::try_from(size).unwrap_or(usize::MAX);
    (values.into_iter().skip(offset).take(limit).collect(), size)
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
