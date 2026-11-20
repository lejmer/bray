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
        let staged_root = (output && subject.subject_root() == DependencySubjectRoot::Result)
            .then(|| self.invocation_result(occurrence).map(Target::storage)).flatten();

        if let Some(root) = &staged_root {
            state.memory.staged_reference_locations(
                occurrence, self.request.semantic_values(), root, root, path,
            )
        } else {
            self.invocation_path_locations(state, occurrence, subject, path)
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
