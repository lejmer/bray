use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Target;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::BoundExecutionSite;
use bray_symbols::DependencySubjectRoot;

pub(super) type StorageAnchors = std::collections::BTreeMap<
    (crate::analysis::storage_flow::memory::Place, bool),
    Vec<(
        crate::analysis::storage_flow::memory::Place,
        Vec<bray_bound_tree::StorageProjection>,
        bool,
    )>,
>;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn storage_anchors(
        &self,
        exit: &crate::analysis::storage_flow::memory::ReferenceExit<'_>,
    ) -> crate::CheckerQueryResult<StorageAnchors, C::UpstreamError> {
        use crate::analysis::storage_flow::memory::{Origin, ReferenceBounds};
        use bray_bound_tree::StorageProjection;
        use bray_symbols::{DependencySubjectRoot, TypeData};

        let mut anchors = StorageAnchors::new();

        for (owner, backing) in exit.owned_backings() {
            if self.formal_root(owner.root).is_none() { continue; }

            let (producer, projections, allocated) = match &backing.origin {
                Origin::Allocation { allocation, projections, .. } => (allocation, projections.to_vec(), true),
                Origin::Returned { reference, projections, .. } => (reference, projections.to_vec(), false),
                _ => continue,
            };

            anchors.entry((producer.clone(), allocated)).or_default()
                .push((owner.clone(), projections, true));
        }

        for (slot, target, exact, bounds) in exit.targets() {
            let Origin::Allocation { allocation, projections, .. } = &target.origin else { continue; };

            if !exact || bounds != ReferenceBounds::Known { continue; }

            let Some(ty) = self.storage.identity_type(slot.root) else { continue; };

            let Some(root) = self.formal_root(slot.root) else { continue; };

            let observable = root == DependencySubjectRoot::Result
                || root.is_static()
                || matches!(self.request.semantic_values().type_data(ty).as_ref(), TypeData::Borrow { .. })
                || slot.projections.contains(&StorageProjection::AddressTarget);

            if !observable || self.reference_kind(slot, ty)? != Some(bray_symbols::DependencyReferenceKind::Address) {
                continue;
            }

            let mut anchor = slot.clone();
            let mut owned = false;

            if let Some(StorageProjection::ProductField(pointer)) = slot.projections.last().copied() {
                let owner = &slot.projections[..slot.projections.len() - 1];

                if let Some((owner_ty, _)) = crate::storage::reference_path(self.request, ty, owner.into(), None)? {
                    let owner_ty = self.request.semantic_values().unborrowed_type(owner_ty);

                    if self.input.raw_buffers.get(&owner_ty).is_some_and(|fields| fields.pointer == pointer) {
                        anchor.projections = owner.into();
                        owned = true;
                    }
                }
            }

            anchors.entry((allocation.clone(), true)).or_default()
                .push((anchor, projections.to_vec(), owned));
        }

        Ok(anchors)
    }

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
