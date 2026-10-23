use super::super::model::{Place, State, Target};
pub(in crate::analysis::storage_flow) enum Source {
    Value(Target),
    Borrow(bray_bound_tree::BorrowCapabilityId),
}

impl Source {
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
