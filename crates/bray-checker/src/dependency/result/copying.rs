use std::collections::BTreeSet;

use bray_bound_tree::BoundExpressionId;
use bray_symbols::{DependencyProjection, DependencyRequirement, DependencyRequirementKind, DependencySubjectRoot};

use crate::{CheckerQueryError, CheckerRequestContext};

use super::core::Transfer;

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
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

            ty = projected.value().expect("checked contents projection retains its selected field type");
        }

        if !self.inputs.value_carries_no_borrows(ty) {
            return Ok(requirements);
        }

        // A contents copy retains carried value dependencies and explicit scope roots.
        // The input's reading lifetime belongs to evaluation, not to the copied value.
        Ok(crate::dependency::witness::map_requirements(
            &requirements.into_iter().collect::<Vec<_>>(),
            &mut |subject, kind, _| {
                let reading_lifetime = kind == DependencyRequirementKind::StorageAlive
                    && matches!(subject.subject_root(), DependencySubjectRoot::Receiver
                        | DependencySubjectRoot::Parameter(_) | DependencySubjectRoot::EvaluationStorage);

                Ok::<_, std::convert::Infallible>(if reading_lifetime {
                    Vec::new()
                } else {
                    vec![DependencyRequirement::direct(subject.clone(), kind)]
                })
            },
        ).expect("copy dependency mapping is infallible").into_iter().collect())
    }
}
