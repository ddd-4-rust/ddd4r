//! Tokio task-local subject binding.

use std::cell::RefCell;
use std::future::Future;

use crate::{AuthError, AuthResult};

#[derive(Debug, Clone, Default)]
struct SubjectContextState {
    token: Option<String>,
}

tokio::task_local! {
    static SUBJECT_CONTEXT: RefCell<SubjectContextState>;
}

/// Runs authentication state inside a cancellation-safe Tokio task scope.
pub struct SubjectScope;

impl SubjectScope {
    /// Runs a future with no initially bound credential.
    pub async fn run<F>(future: F) -> F::Output
    where
        F: Future,
    {
        SUBJECT_CONTEXT
            .scope(RefCell::new(SubjectContextState::default()), future)
            .await
    }

    /// Runs a future with an already verified transport credential.
    pub async fn run_with_token<F>(token: impl Into<String>, future: F) -> F::Output
    where
        F: Future,
    {
        SUBJECT_CONTEXT
            .scope(
                RefCell::new(SubjectContextState {
                    token: Some(token.into()),
                }),
                future,
            )
            .await
    }

    /// Runs a nested scope that inherits but cannot mutate its parent's binding.
    pub async fn nested<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let inherited = SUBJECT_CONTEXT
            .try_with(|context| context.borrow().clone())
            .unwrap_or_default();
        SUBJECT_CONTEXT.scope(RefCell::new(inherited), future).await
    }

    /// Binds a verified token in the current task scope.
    pub fn bind(token: impl Into<String>) -> AuthResult<()> {
        SUBJECT_CONTEXT
            .try_with(|context| context.borrow_mut().token = Some(token.into()))
            .map_err(|_| AuthError::Infrastructure {
                message: "SubjectScope is not active".to_owned(),
            })
    }

    pub(crate) fn try_bind(token: impl Into<String>) -> bool {
        SUBJECT_CONTEXT
            .try_with(|context| context.borrow_mut().token = Some(token.into()))
            .is_ok()
    }

    /// Clears the current task's binding.
    pub fn clear() {
        let _ = SUBJECT_CONTEXT.try_with(|context| context.borrow_mut().token = None);
    }

    /// Returns the current task's opaque credential without validating it.
    pub fn current_token() -> Option<String> {
        SUBJECT_CONTEXT
            .try_with(|context| context.borrow().token.clone())
            .ok()
            .flatten()
    }
}
