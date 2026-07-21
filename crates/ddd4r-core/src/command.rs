//! CQRS command routing.

use std::any::{Any, TypeId, type_name};
use std::marker::PhantomData;
use std::sync::Arc;

use dashmap::DashMap;
use futures::future::BoxFuture;

use crate::{DddError, DddResult};

/// Marker contract for commands.
pub trait Command: Any + Send + Sync + 'static {}

impl<T> Command for T where T: Any + Send + Sync + 'static {}

/// Typed command executor.
pub trait CommandExecutor<C>: Send + Sync + 'static
where
    C: Command,
{
    /// Result produced by this executor.
    type Output: Any + Send + Sync + 'static;

    /// Executes a command.
    fn execute<'a>(&'a self, command: &'a C) -> BoxFuture<'a, DddResult<Self::Output>>;
}

trait ErasedCommandExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: &'a (dyn Any + Send + Sync),
    ) -> BoxFuture<'a, DddResult<Box<dyn Any + Send + Sync>>>;
}

struct ExecutorAdapter<C, E> {
    executor: E,
    marker: PhantomData<fn() -> C>,
}

impl<C, E> ErasedCommandExecutor for ExecutorAdapter<C, E>
where
    C: Command,
    E: CommandExecutor<C>,
{
    fn execute<'a>(
        &'a self,
        command: &'a (dyn Any + Send + Sync),
    ) -> BoxFuture<'a, DddResult<Box<dyn Any + Send + Sync>>> {
        Box::pin(async move {
            let typed = command
                .downcast_ref::<C>()
                .ok_or(DddError::CommandResultTypeMismatch {
                    expected: type_name::<C>(),
                })?;
            let result = self.executor.execute(typed).await?;
            let erased: Box<dyn Any + Send + Sync> = Box::new(result);
            Ok(erased)
        })
    }
}

/// Object-safe command bus boundary.
pub trait CommandBus: Send + Sync + 'static {
    /// Executes an erased command.
    fn execute_boxed<'a>(
        &'a self,
        command_type: TypeId,
        command_name: &'static str,
        command: &'a (dyn Any + Send + Sync),
    ) -> BoxFuture<'a, DddResult<Box<dyn Any + Send + Sync>>>;
}

/// Explicit, duplicate-rejecting command router.
#[derive(Default)]
pub struct DefaultCommandBus {
    executors: DashMap<TypeId, Arc<dyn ErasedCommandExecutor>>,
}

impl DefaultCommandBus {
    /// Creates an empty command bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one executor for a command type.
    pub fn register<C, E>(&self, executor: E) -> DddResult<()>
    where
        C: Command,
        E: CommandExecutor<C>,
    {
        let adapter: Arc<dyn ErasedCommandExecutor> = Arc::new(ExecutorAdapter::<C, E> {
            executor,
            marker: PhantomData,
        });
        match self.executors.entry(TypeId::of::<C>()) {
            dashmap::mapref::entry::Entry::Occupied(_) => Err(DddError::MultipleCommandExecutors {
                command: type_name::<C>(),
            }),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(adapter);
                Ok(())
            }
        }
    }

    /// Executes a typed command and downcasts its typed result.
    pub async fn execute<C, R>(&self, command: &C) -> DddResult<R>
    where
        C: Command,
        R: Any + Send + Sync + 'static,
    {
        let result = self
            .execute_boxed(TypeId::of::<C>(), type_name::<C>(), command)
            .await?;
        result.downcast::<R>().map(|value| *value).map_err(|_| {
            DddError::CommandResultTypeMismatch {
                expected: type_name::<R>(),
            }
        })
    }

    /// Returns the number of registered command types.
    pub fn len(&self) -> usize {
        self.executors.len()
    }

    /// Returns whether no command executors are registered.
    pub fn is_empty(&self) -> bool {
        self.executors.is_empty()
    }
}

impl CommandBus for DefaultCommandBus {
    fn execute_boxed<'a>(
        &'a self,
        command_type: TypeId,
        command_name: &'static str,
        command: &'a (dyn Any + Send + Sync),
    ) -> BoxFuture<'a, DddResult<Box<dyn Any + Send + Sync>>> {
        Box::pin(async move {
            let executor = self
                .executors
                .get(&command_type)
                .map(|entry| Arc::clone(entry.value()));
            let executor = executor.ok_or(DddError::CommandExecutorNotFound {
                command: command_name,
            })?;
            executor.execute(command).await
        })
    }
}
