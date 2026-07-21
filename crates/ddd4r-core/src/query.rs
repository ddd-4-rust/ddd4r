//! Strongly typed query abstract syntax tree.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::DddResult;
use crate::domain::AggregateRoot;
use crate::repository::RepositoryRegistry;

/// Comparison and collection operators supported by repository adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// Equality.
    Eq,
    /// Inequality.
    Ne,
    /// Greater-than.
    Gt,
    /// Greater-than or equal.
    Ge,
    /// Less-than.
    Lt,
    /// Less-than or equal.
    Le,
    /// SQL-like pattern match.
    Like,
    /// SQL-like suffix match (`%value`).
    LikeLeft,
    /// SQL-like prefix match (`value%`).
    LikeRight,
    /// Negated SQL-like pattern match.
    NotLike,
    /// Membership in a set.
    In,
    /// Exclusion from a set.
    NotIn,
    /// Null check.
    IsNull,
    /// Non-null check.
    IsNotNull,
}

/// A generated reference to a field of an aggregate or persistence object.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PropertyRef<A, V> {
    name: &'static str,
    marker: PhantomData<fn(A) -> V>,
}

impl<A, V> Copy for PropertyRef<A, V> {}

impl<A, V> Clone for PropertyRef<A, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A, V> PropertyRef<A, V>
where
    V: Serialize,
{
    /// Creates a generated field reference.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            marker: PhantomData,
        }
    }

    /// Returns the stable persistence property name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Creates an equality condition.
    pub fn eq(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Eq, value)
    }

    /// Creates an inequality condition.
    pub fn ne(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Ne, value)
    }

    /// Creates a greater-than condition.
    pub fn gt(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Gt, value)
    }

    /// Creates a greater-than-or-equal condition.
    pub fn ge(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Ge, value)
    }

    /// Creates a less-than condition.
    pub fn lt(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Lt, value)
    }

    /// Creates a less-than-or-equal condition.
    pub fn le(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Le, value)
    }

    /// Creates a SQL-like contains condition.
    pub fn like(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::Like, value)
    }

    /// Creates a SQL-like suffix condition.
    pub fn like_left(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::LikeLeft, value)
    }

    /// Creates a SQL-like prefix condition.
    pub fn like_right(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::LikeRight, value)
    }

    /// Creates a negated SQL-like condition.
    pub fn not_like(self, value: V) -> DddResult<Condition> {
        Condition::single(self.name, Operator::NotLike, value)
    }

    /// Creates a set-membership condition.
    pub fn is_in<I>(self, values: I) -> DddResult<Condition>
    where
        I: IntoIterator<Item = V>,
    {
        Condition::many(self.name, Operator::In, values)
    }

    /// Creates a set-exclusion condition.
    pub fn not_in<I>(self, values: I) -> DddResult<Condition>
    where
        I: IntoIterator<Item = V>,
    {
        Condition::many(self.name, Operator::NotIn, values)
    }

    /// Creates a null check.
    pub fn is_null(self) -> Condition {
        Condition::null(self.name, Operator::IsNull)
    }

    /// Creates a non-null check.
    pub fn is_not_null(self) -> Condition {
        Condition::null(self.name, Operator::IsNotNull)
    }
}

/// One predicate in a framework-neutral query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    /// Stable persistence property.
    pub property: String,
    /// Predicate operator.
    pub operator: Operator,
    /// Canonically serialized operands.
    pub operands: Vec<Value>,
}

impl Condition {
    /// Builds a one-operand condition.
    pub fn single<T>(property: impl Into<String>, operator: Operator, operand: T) -> DddResult<Self>
    where
        T: Serialize,
    {
        Ok(Self {
            property: property.into(),
            operator,
            operands: vec![serde_json::to_value(operand)?],
        })
    }

    /// Builds a multi-operand condition.
    pub fn many<T, I>(
        property: impl Into<String>,
        operator: Operator,
        operands: I,
    ) -> DddResult<Self>
    where
        T: Serialize,
        I: IntoIterator<Item = T>,
    {
        let operands = operands
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            property: property.into(),
            operator,
            operands,
        })
    }

    /// Builds a condition that has no operands, such as `IS NULL`.
    pub fn null(property: impl Into<String>, operator: Operator) -> Self {
        Self {
            property: property.into(),
            operator,
            operands: Vec::new(),
        }
    }
}

/// Sort direction for a query property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Stable persistence property.
    pub property: String,
    /// `true` for ascending order.
    pub ascending: bool,
}

impl Order {
    /// Creates ascending ordering.
    pub fn asc(property: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            ascending: true,
        }
    }

    /// Creates descending ordering.
    pub fn desc(property: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            ascending: false,
        }
    }
}

/// One-based page request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    /// One-based page number.
    pub current: u64,
    /// Page size. `None` means unpaged.
    pub size: Option<u64>,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            current: 1,
            size: Some(20),
        }
    }
}

/// A page of aggregate results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Returned records.
    pub records: Vec<T>,
    /// Total matching records.
    pub total: u64,
    /// One-based page number.
    pub current: u64,
    /// Requested page size.
    pub size: u64,
}

impl<T> Page<T> {
    /// Creates an empty page.
    pub const fn empty(current: u64, size: u64) -> Self {
        Self {
            records: Vec::new(),
            total: 0,
            current,
            size,
        }
    }

    /// Returns whether this page has no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Framework-neutral query with ddd4j-compatible rich execution methods.
#[derive(Debug, Clone)]
pub struct Query<A> {
    /// Conjunctive where predicates.
    pub conditions: Vec<Condition>,
    /// Sort expressions in priority order.
    pub orders: Vec<Order>,
    /// Update `SET` operations for conditional updates.
    pub set_operations: Vec<Condition>,
    /// Selected persistence properties.
    pub select_columns: Vec<String>,
    /// Grouping persistence properties.
    pub group_by_columns: Vec<String>,
    /// Optional adapter-native HAVING expression.
    pub having: Option<String>,
    /// Aggregate-fill relation names.
    pub fills: Vec<String>,
    /// Pagination.
    pub page: PageRequest,
    /// Whether tenant filtering is explicitly disabled.
    pub ignore_tenant: bool,
    marker: PhantomData<fn() -> A>,
}

impl<A> Default for Query<A> {
    fn default() -> Self {
        Self {
            conditions: Vec::new(),
            orders: Vec::new(),
            set_operations: Vec::new(),
            select_columns: Vec::new(),
            group_by_columns: Vec::new(),
            having: None,
            fills: Vec::new(),
            page: PageRequest::default(),
            ignore_tenant: false,
            marker: PhantomData,
        }
    }
}

impl<A> Query<A>
where
    A: AggregateRoot,
{
    /// Creates an empty query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a conjunctive condition.
    pub fn and(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Adds a condition only when `enabled` is true.
    pub fn and_if(self, enabled: bool, condition: Condition) -> Self {
        if enabled { self.and(condition) } else { self }
    }

    /// Adds an ordering expression.
    pub fn order_by(mut self, order: Order) -> Self {
        self.orders.push(order);
        self
    }

    /// Adds a conditional update operation.
    pub fn set(mut self, operation: Condition) -> Self {
        self.set_operations.push(operation);
        self
    }

    /// Selects an explicit set of persistence properties.
    pub fn select<I, S>(mut self, columns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.select_columns
            .extend(columns.into_iter().map(Into::into));
        self
    }

    /// Groups by an explicit set of persistence properties.
    pub fn group_by<I, S>(mut self, columns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.group_by_columns
            .extend(columns.into_iter().map(Into::into));
        self
    }

    /// Sets an adapter-native HAVING expression.
    pub fn having(mut self, expression: impl Into<String>) -> Self {
        self.having = Some(expression.into());
        self
    }

    /// Requests aggregate-fill relations after the primary query.
    pub fn fills<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.fills.extend(names.into_iter().map(Into::into));
        self
    }

    /// Returns whether an aggregate-fill relation was requested.
    pub fn has_fill(&self, name: &str) -> bool {
        self.fills.iter().any(|candidate| candidate == name)
    }

    /// Sets one-based pagination.
    pub const fn page(mut self, current: u64, size: u64) -> Self {
        self.page = PageRequest {
            current: if current == 0 { 1 } else { current },
            size: Some(size),
        };
        self
    }

    /// Disables pagination.
    pub const fn unpaged(mut self) -> Self {
        self.page.size = None;
        self
    }

    /// Explicitly bypasses tenant filtering.
    pub const fn ignoring_tenant(mut self) -> Self {
        self.ignore_tenant = true;
        self
    }

    /// Executes a list query through the effective repository.
    pub async fn list(&self) -> DddResult<Vec<A>> {
        RepositoryRegistry::repository::<A>()?.find_list(self).await
    }

    /// Executes a page query through the effective repository.
    pub async fn list_page(&self) -> DddResult<Page<A>> {
        RepositoryRegistry::repository::<A>()?.page(self).await
    }

    /// Returns the first matching aggregate.
    pub async fn one(&self) -> DddResult<Option<A>> {
        RepositoryRegistry::repository::<A>()?
            .find_first(self)
            .await
    }

    /// Counts matching aggregates.
    pub async fn count(&self) -> DddResult<u64> {
        RepositoryRegistry::repository::<A>()?.count(self).await
    }

    /// Returns whether any aggregate matches.
    pub async fn exists(&self) -> DddResult<bool> {
        RepositoryRegistry::repository::<A>()?.exists(self).await
    }

    /// Returns map-shaped projection rows.
    pub async fn maps(&self) -> DddResult<Vec<crate::repository::RepositoryRow>> {
        RepositoryRegistry::repository::<A>()?.maps(self).await
    }

    /// Returns the first map-shaped projection row.
    pub async fn map(&self) -> DddResult<Option<crate::repository::RepositoryRow>> {
        Ok(self.maps().await?.into_iter().next())
    }

    /// Deletes aggregates matching this query.
    pub async fn delete(&self) -> DddResult<bool> {
        RepositoryRegistry::repository::<A>()?
            .delete_by_query(self)
            .await
    }

    /// Updates aggregates matching this query.
    pub async fn update(&self, aggregate: &A) -> DddResult<bool> {
        RepositoryRegistry::repository::<A>()?
            .update_where(aggregate, self)
            .await
    }

    /// Applies configured aggregate fills to the supplied models.
    pub async fn do_fills(&self, aggregates: &mut [A]) -> DddResult<()> {
        RepositoryRegistry::repository::<A>()?
            .fill_many(self, aggregates)
            .await
    }
}
