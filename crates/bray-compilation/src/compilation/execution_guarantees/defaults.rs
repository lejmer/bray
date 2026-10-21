use std::collections::BTreeMap;

use bray_bound_tree::BodyBehaviorContributions;
use bray_checker::ResultDependencyOwner;
use bray_symbols::DependencyContractTemplateId;

use crate::compilation::{Compilation, binder};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    /// Supplies settled defaults only outside a source component's in-progress inference.
    pub(in crate::compilation) fn execution_default_inputs(
        &self,
        behavior: &BodyBehaviorContributions,
        cancellation: &CancellationToken,
    ) -> Result<BTreeMap<ResultDependencyOwner, DependencyContractTemplateId>, FactQueryError> {
        let context = self.binding_context(cancellation)?;

        behavior.defaults().iter().map(|provider| {
            let summary = binder::default_result_dependencies(&context, *provider)
                .map_err(binder::binding_query_error)?;
            Ok((ResultDependencyOwner::Declaration(provider.symbol()), summary))
        }).collect()
    }
}
