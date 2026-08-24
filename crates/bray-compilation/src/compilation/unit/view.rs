use std::sync::Arc;

use bray_bound_tree::{
    CheckedAsync, CheckedBodySemantics, CheckedDependencyContracts, CheckedExpressionSemantics,
    CheckedExpressionTypes, CheckedLiteralValues, CheckedRefinements, CheckedSemanticSelections,
    Liveness, StorageFlow,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

macro_rules! semantic_view {
    ($name:ident, $owner:ty, $value:ty, $project:ident) => {
        #[doc = concat!("A stable immutable view of `", stringify!($value), "` and its stage diagnostics.")]
        #[derive(Clone, Debug)]
        pub struct $name {
            owner: Arc<DiagnosticResult<$owner>>,
        }

        impl $name {
            pub(in crate::compilation) const fn new(
                owner: Arc<DiagnosticResult<$owner>>,
            ) -> Self {
                Self { owner }
            }

            /// Returns the focused semantic value without copying its owning stage.
            pub fn value(&self) -> &$value {
                self.owner.value().$project()
            }

            /// Returns the diagnostics owned by this semantic stage.
            pub fn diagnostics(&self) -> &DiagnosticBag {
                self.owner.diagnostics()
            }

            #[cfg(test)]
            pub(crate) fn snapshot_address(&self) -> *const () {
                Arc::as_ptr(&self.owner).cast()
            }
        }
    };
}

semantic_view!(
    ExpressionTypesView,
    CheckedExpressionSemantics,
    CheckedExpressionTypes,
    types
);

semantic_view!(
    LiteralValuesView,
    CheckedExpressionSemantics,
    CheckedLiteralValues,
    literals
);

semantic_view!(
    SemanticSelectionsView,
    CheckedExpressionSemantics,
    CheckedSemanticSelections,
    selections
);

semantic_view!(
    LivenessView,
    CheckedBodySemantics,
    Liveness,
    liveness
);

semantic_view!(
    RefinementsView,
    CheckedBodySemantics,
    CheckedRefinements,
    refinements
);

semantic_view!(
    StorageFlowView,
    CheckedBodySemantics,
    StorageFlow,
    storage_flow
);

semantic_view!(
    DependencyContractsView,
    CheckedBodySemantics,
    CheckedDependencyContracts,
    dependencies
);

semantic_view!(
    AsyncAnalysisView,
    CheckedBodySemantics,
    CheckedAsync,
    asynchronous
);
