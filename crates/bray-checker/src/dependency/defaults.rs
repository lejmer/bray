use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::{SelectedArgument, SelectedCall};
use bray_symbols::{
    DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
    DependencySubjectRoot,
};

/// Replaces omitted inputs with their declaration-owned provider dependencies.
pub(crate) fn expand_result_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    template: &DependencyContractTemplateData,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let mut result = Vec::new();
    let mut pending = template.requirements().to_vec();
    let mut visited = std::collections::BTreeSet::new();

    while let Some(requirement) = pending.pop() {
        let DependencyRequirement::Direct { subject, .. } = &requirement else {
            if let DependencyRequirement::Guarded(guarded) = requirement {
                pending.extend(guarded.requirements().iter().cloned());
            }

            continue;
        };

        let default = match subject.subject_root() {
            DependencySubjectRoot::Parameter(ordinal) => {
                call.arguments().iter().find_map(|argument| match argument {
                    SelectedArgument::Default {
                        parameter,
                        ordinal: actual,
                        ..
                    } if *actual == ordinal.raw() => Some(*parameter),
                    _ => None,
                })
            }
            _ => None,
        };

        let Some(parameter) = default else {
            result.push(requirement);
            continue;
        };

        if !visited.insert(parameter) {
            continue;
        }

        let template = request
            .context()
            .parameter_default_dependencies(parameter)?;

        let template = request
            .semantic_values()
            .dependency_contract_template_data(template)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        pending.extend(
            template
                .requirements()
                .iter()
                .filter(|requirement| {
                    matches!(
                        requirement,
                        DependencyRequirement::Direct {
                            kind: DependencyRequirementKind::ValueDependencies,
                            ..
                        }
                    )
                })
                .cloned(),
        );
    }

    Ok(DependencyContractTemplateData::new(result))
}
