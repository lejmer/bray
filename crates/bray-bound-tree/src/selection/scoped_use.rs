use bray_symbols::{CallableInstanceData, CallableSignature, TypeId};

use crate::{BoundCallResult, BoundExpressionId};

/// The matched lifecycle declarations selected for one scoped-use occurrence.
/// The initializer determines selection and the successful enter result determines the pattern subject.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedScopedUse {
    expression: BoundExpressionId,
    initializer: BoundExpressionId,
    source_type: TypeId,
    capability_type: TypeId,
    failure_type: Option<TypeId>,
    enter: (CallableInstanceData, CallableSignature),
    exit: (CallableInstanceData, CallableSignature),
    enter_result: BoundCallResult,
    exit_result: BoundCallResult,
}

impl SelectedScopedUse {
    /// Creates a matched pair after checking the receiver and capability relationship.
    #[expect(
        clippy::too_many_arguments,
        reason = "selected scoped use retains both closed invocations and their execution results"
    )]
    pub fn new(
        expression: BoundExpressionId,
        initializer: BoundExpressionId,
        source_type: TypeId,
        capability_type: TypeId,
        failure_type: Option<TypeId>,
        enter: (CallableInstanceData, CallableSignature),
        exit: (CallableInstanceData, CallableSignature),
        enter_result: BoundCallResult,
        exit_result: BoundCallResult,
    ) -> Self {
        assert_eq!(
            expression.unit(),
            initializer.unit(),
            "scoped-use initializer belongs to its expression unit"
        );

        assert!(
            enter.1.receiver().is_some(),
            "selected scope enter retains its receiver"
        );

        assert!(
            exit.1.receiver().is_none(),
            "selected scope exit has no receiver"
        );

        assert_eq!(
            exit.1.parameters().len(),
            1,
            "selected scope exit retains its sole capability parameter"
        );

        Self {
            expression,
            initializer,
            source_type,
            capability_type,
            failure_type,
            enter,
            exit,
            enter_result,
            exit_result,
        }
    }

    /// Returns the actual with-expression occurrence.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the initializer evaluated before entering scoped use.
    pub const fn initializer(&self) -> BoundExpressionId {
        self.initializer
    }

    /// Returns the initializer's checked value or access-path type.
    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    /// Returns the successful capability type matched by the irrefutable pattern.
    pub const fn capability_type(&self) -> TypeId {
        self.capability_type
    }

    /// Returns the error type contributed by the selected entry and exit declarations.
    pub const fn failure_type(&self) -> Option<TypeId> {
        self.failure_type
    }

    /// Returns the selected enter instance and its substituted declaration signature.
    pub const fn enter(&self) -> &(CallableInstanceData, CallableSignature) {
        &self.enter
    }

    /// Returns the matching exit instance and its substituted declaration signature.
    pub const fn exit(&self) -> &(CallableInstanceData, CallableSignature) {
        &self.exit
    }

    /// Returns the immediate value or lazy computation created by enter.
    pub const fn enter_result(&self) -> BoundCallResult {
        self.enter_result
    }

    /// Returns the immediate value or lazy computation created by exit.
    pub const fn exit_result(&self) -> BoundCallResult {
        self.exit_result
    }

    /// Returns the closed callable, signature and execution result for one actual protocol phase.
    pub fn invocation(
        &self,
        occurrence: crate::BoundExecutionSite,
    ) -> (CallableInstanceData, &CallableSignature, BoundCallResult) {
        assert_eq!(
            occurrence.expression(),
            Some(self.expression),
            "scoped invocation belongs to its selected with occurrence"
        );

        match occurrence {
            crate::BoundExecutionSite::ScopedEnter(_) => {
                (self.enter.0, &self.enter.1, self.enter_result)
            }
            crate::BoundExecutionSite::ScopedExit(_) => {
                (self.exit.0, &self.exit.1, self.exit_result)
            }
            crate::BoundExecutionSite::Node(_) => {
                panic!("scoped invocation retains its enter or exit phase")
            }
        }
    }
}
