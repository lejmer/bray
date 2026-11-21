use std::collections::BTreeSet;

use bray_bound_tree::BoundExpressionId;
use bray_symbols::{DependencyProjection, DependencyRequirement, DependencyRequirementKind, DependencySubjectRoot};

use crate::{CheckerQueryError, CheckerRequestContext};

use super::core::Transfer;

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
    pub(crate) fn propagate_failure(
        &mut self,
        expression: BoundExpressionId,
        region: Option<BoundExpressionId>,
    ) {
        let requirements = self.inputs.error_exits.get(&expression).map_or_else(BTreeSet::new, |(value, projection, ty)| {
            let requirements = self.projected_values(*value, std::slice::from_ref(projection));

            self.retain_copied_values(requirements, *ty)
        });

        if let Some(region) = region {
            super::state::assign(&mut self.state.values, region.into(), requirements);
        } else {
            self.state.returned = requirements;
        }
    }

    pub(super) fn copied_values(
        &self,
        value: BoundExpressionId,
        path: &[DependencyProjection],
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let requirements = self.projected_values(value, path);

        if path.is_empty() {
            return Ok(requirements);
        }

        let mut ty = self.types.expression(value)
            .expect("checked contents projection retains its source type").ty();

        for projection in path {
            let projected = crate::storage::projected_value_type(self.request, ty, *projection)?;

            ty = projected.value().unwrap_or_else(|| panic!(
                "checked dependency copy projection has no selected type: expression={value:?}, node={:?}, owner={:?}, projection={projection:?}, path={path:?}",
                self.request.view().expression(value), self.request.semantic_values().type_data(ty),
            ));
        }

        Ok(self.retain_copied_values(requirements, ty))
    }

    pub(super) fn retain_copied_values(
        &self,
        requirements: BTreeSet<DependencyRequirement>,
        ty: bray_symbols::TypeId,
    ) -> BTreeSet<DependencyRequirement> {
        if !self.inputs.value_carries_no_borrows(ty) {
            return requirements;
        }

        // A contents copy retains carried value dependencies and explicit scope roots.
        // The input's reading lifetime belongs to evaluation, not to the copied value.
        crate::dependency::witness::map_requirements(
            &requirements.into_iter().collect::<Vec<_>>(),
            &mut |subject, kind, _| {
                let reading_lifetime = matches!(kind, DependencyRequirementKind::StorageAlive
                    | DependencyRequirementKind::BorrowCapabilityActive(_))
                    && matches!(subject.subject_root(), DependencySubjectRoot::Receiver
                        | DependencySubjectRoot::Parameter(_) | DependencySubjectRoot::EvaluationStorage);

                Ok::<_, std::convert::Infallible>(if reading_lifetime {
                    Vec::new()
                } else {
                    vec![DependencyRequirement::direct(subject.clone(), kind)]
                })
            },
        ).expect("copy dependency mapping is infallible").into_iter().collect()
    }
}
