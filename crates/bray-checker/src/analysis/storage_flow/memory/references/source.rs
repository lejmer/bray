use super::super::model::{Place, State, Target};
#[derive(Clone)]
pub(in crate::analysis::storage_flow) enum Source {
    Value(Target),
    Borrow(bray_bound_tree::BorrowCapabilityId),
}

impl Source {
    pub(in crate::analysis::storage_flow) fn is_exact(&self, state: &State) -> bool {
        match self {
            Self::Value(source) => state.values.get(source).and_then(|value| value.references.as_ref()),
            Self::Borrow(capability) => state.borrows.get(capability),
        }
        .is_some_and(super::super::model::References::is_exact)
    }

    pub(in crate::analysis::storage_flow) fn stage(
        self,
        state: &mut State,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        match self {
            Self::Value(source) => state.stage_value(call, &source, destination),
            Self::Borrow(capability) => state.stage_borrow(call, capability, destination),
        }
    }

    pub(in crate::analysis::storage_flow) fn targets(&self, state: &State) -> Option<Vec<Target>> {
        match self {
            Self::Value(source) => state.value_targets(source),
            Self::Borrow(capability) => state.borrow_targets(*capability),
        }
    }

    pub(in crate::analysis::storage_flow) fn address_targets(&self, state: &State) -> Option<Vec<Target>> {
        let references = match self {
            Self::Value(source) => state.values.get(source)?.references.as_ref()?,
            Self::Borrow(capability) => state.borrows.get(capability)?,
        };

        Some(references.address_targets()?.iter().cloned().collect())
    }

    pub(in crate::analysis::storage_flow) fn reference_locations(
        &self,
        values: &bray_symbols::SemanticValueStore,
        state: &State,
        path: &bray_bound_tree::StorageReferencePath,
        regions: bool,
    ) -> Option<Vec<Target>> {
        let resolve = |target, root| {
            if regions {
                state.reference_target_regions(values, target, path, root)
            } else {
                state.reference_locations(values, target, path, root)
            }
        };

        match self {
            Self::Value(source) => resolve(source.clone(), None),
            Self::Borrow(_) => {
                let targets = if regions && path.projections().is_empty() && path.borrows().len() == 1 {
                    self.regions(state)?
                } else {
                    self.address_targets(state)?
                };

                targets.into_iter().map(|target| resolve(target.clone(), Some(target)))
                    .collect::<Option<Vec<_>>>().map(|targets| targets.into_iter().flatten().collect())
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn regions(&self, state: &State) -> Option<Vec<Target>> {
        match self {
            Self::Value(source) => state.referenced_regions(source),
            Self::Borrow(capability) => state.borrow_regions(*capability),
        }
    }

    pub(in crate::analysis::storage_flow) fn transfer(self, state: &mut State, destination: Place) {
        match self {
            Self::Value(source) => state.transfer_value(&source, destination),
            Self::Borrow(capability) => state.retain_borrow(capability, destination),
        }
    }
}
