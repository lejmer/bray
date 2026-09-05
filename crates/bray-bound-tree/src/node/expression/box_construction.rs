use std::sync::Arc;

use crate::{BoundArgument, BoundNodeOrigin, BoundTypeReference};
use bray_base::shared_slice;

/// An owned-indirection construction with its declared policy and source-ordered arguments.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundBoxConstructionExpression {
    origin: BoundNodeOrigin,
    policy: Option<BoundTypeReference>,
    arguments: Arc<[BoundArgument]>,
    is_recovered: bool,
}

impl BoundBoxConstructionExpression {
    /// Retains the optional storage policy without treating it as a runtime operand.
    pub fn new(
        origin: BoundNodeOrigin,
        policy: Option<BoundTypeReference>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            policy,
            arguments: shared_slice(arguments),
            is_recovered,
        }
    }

    /// Returns the construction's source origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the explicitly written storage policy, if any.
    pub const fn policy(&self) -> Option<BoundTypeReference> {
        self.policy
    }

    /// Returns runtime arguments in evaluation order, including their names.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }

    /// Returns whether source recovery affected this construction.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}
