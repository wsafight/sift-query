use super::super::plan::ServerScope;
use crate::{ParamError, SearchEntity};

/// Builds a server-owned authorization scope from a trusted request context.
///
/// The context is supplied by the application after authentication. It is never
/// parsed from `RawParams`, and the resulting scope is still validated by the
/// normal query planner before any SQL is rendered.
pub trait ScopeProvider<C, E: SearchEntity>: Send + Sync {
    fn scope(&self, context: &C) -> Result<ServerScope<E>, ParamError>;
}

impl<C, E, F> ScopeProvider<C, E> for F
where
    E: SearchEntity,
    F: Fn(&C) -> Result<ServerScope<E>, ParamError> + Send + Sync,
{
    fn scope(&self, context: &C) -> Result<ServerScope<E>, ParamError> {
        self(context)
    }
}
