use std::collections::BTreeSet;

use bray_bound_tree::BoundUnit;
use bray_diagnostics::DiagnosticResult;

use crate::BinderDependency;

/// One complete diagnostic-bearing binder computation and its dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundUnitComputation {
    result: DiagnosticResult<BoundUnit>,
    dependencies: Box<[BinderDependency]>,
}

impl BoundUnitComputation {
    /// Creates a complete computation with canonical deterministic dependencies.
    pub fn new(
        result: DiagnosticResult<BoundUnit>,
        dependencies: impl IntoIterator<Item = BinderDependency>,
    ) -> Self {
        let dependencies = dependencies
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            result,
            dependencies,
        }
    }

    /// Returns the atomic diagnostic-bearing value.
    pub const fn result(&self) -> &DiagnosticResult<BoundUnit> {
        &self.result
    }

    /// Returns every semantic dependency observed by the computation in canonical order.
    pub fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }

    /// Consumes the computation into its result and dependencies.
    pub fn into_parts(self) -> (DiagnosticResult<BoundUnit>, Box<[BinderDependency]>) {
        (self.result, self.dependencies)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::BoundUnitComputation;
    use crate::BindingOutcome;

    #[test]
    fn binding_outcomes_publish_values_and_diagnostics_together() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(5),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let outcome = BindingOutcome::complete(7_u32, DiagnosticBag::single(diagnostic.clone()));

        let Some(result) = outcome.result() else {
            panic!("completed binding must retain its atomic result");
        };

        assert_eq!(result.value(), &7);

        assert_eq!(
            result.diagnostics().iter().cloned().collect::<Vec<_>>(),
            [diagnostic]
        );
    }

    #[test]
    fn binding_cancellation_exposes_no_partial_result() {
        let outcome = BindingOutcome::<u32>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn bound_unit_computations_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<BoundUnitComputation>();
    }
}
