use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Target;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::BoundExecutionSite;
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_stored_locations(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        subject: &bray_symbols::DependencySubject,
        path: &bray_bound_tree::StorageReferencePath,
        output: bool,
    ) -> Option<Vec<Target>> {
        let root = subject.subject_root();
        let values = self.request.semantic_values();

        if root.is_static() {
            let target = self.static_location(&bray_symbols::DependencySubject::root(root))?;

            return state.memory.reference_locations(values, target, path, None);
        }

        assert_eq!(root, DependencySubjectRoot::Result, "an operand's route starts at its admitted capture");

        let result = Target::storage(self.invocation_result(occurrence)?);

        if output {
            state.memory.staged_reference_locations(occurrence, values, &result, &result, path)
        } else {
            state.memory.reference_locations(values, result, path, None)
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_stored_root(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Target> {
        if root == DependencySubjectRoot::Result {
            self.invocation_result(occurrence).map(Target::storage)
        } else {
            self.invocation_place(occurrence, root).map(Target::storage)
                .or_else(|| self.static_location(&bray_symbols::DependencySubject::root(root)))
        }
    }
}
