use super::super::model::{Place, State, Target};
#[derive(Clone)]
pub(in crate::analysis::storage_flow) enum Source {
    Value(Target),
    Borrow(bray_bound_tree::BorrowCapabilityId),
    Captured(bray_bound_tree::StorageCapturedValue),
}

impl Source {
    pub(in crate::analysis::storage_flow) fn reference_kind(
        &self,
        state: &State,
    ) -> bray_symbols::DependencyReferenceKind {
        match self {
            Self::Value(source) => state
                .reached_route_binding(source, None)
                .map_or(bray_symbols::DependencyReferenceKind::Address, |binding| {
                    binding.reference_kind()
                }),
            Self::Borrow(_) => bray_symbols::DependencyReferenceKind::Borrow,
            Self::Captured(source) => source
                .binding()
                .map_or(bray_symbols::DependencyReferenceKind::Address, |binding| {
                    binding.reference_kind()
                }),
        }
    }

    pub(in crate::analysis::storage_flow) fn is_exact(&self, state: &State) -> bool {
        let exact = |address: Option<&super::super::model::AddressValue>| {
            address.is_some_and(super::super::model::AddressValue::is_exact)
        };

        match self {
            Self::Value(source) => state.reached_route_binding(source, None)
                .is_some_and(|value| exact(value.address.as_ref())),
            Self::Borrow(capability) => exact(state.borrow_address(*capability)),
            Self::Captured(source) => exact(source.binding().and_then(|value| value.address.as_ref())),
        }
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
            Self::Captured(source) => {
                let captured = source.materialize(&destination);

                state.stage_captured_value(call, destination, captured);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn targets(&self, state: &State) -> Option<Vec<Target>> {
        match self {
            Self::Value(source) => state.value_targets(source),
            Self::Borrow(capability) => state.borrow_targets(*capability),
            Self::Captured(source) => source.binding()?.address.as_ref()?.actual_targets(),
        }
    }

    pub(in crate::analysis::storage_flow) fn regions(&self, state: &State) -> Option<Vec<Target>> {
        match self {
            Self::Value(source) => state.referenced_regions(source),
            Self::Borrow(capability) => state.borrow_regions(*capability),
            Self::Captured(source) => source.address_regions(),
        }
    }

    pub(in crate::analysis::storage_flow) fn mutation_targets(
        &self,
        state: &State,
    ) -> Option<Vec<super::super::MutationTarget>> {
        match self {
            Self::Value(source) => state.reached_route_binding(source, None)?.mutation_targets(),
            Self::Captured(source) => source.mutation_targets(),
            Self::Borrow(capability) => {
                let address = state.borrow_address(*capability)?;

                Some(super::super::MutationTarget::reference(Some(address), address.regions()?))
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn transfer(self, state: &mut State, destination: Place) {
        match self {
            Self::Value(source) => state.transfer_value(&source, destination),
            Self::Borrow(capability) => state.retain_borrow(capability, destination),
            Self::Captured(source) => state.publish_captured_value(&Target::storage(destination), &source),
        }
    }
}
