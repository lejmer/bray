use std::collections::BTreeSet;

use crate::{
    RuntimeArtifactComponentMetadata, RuntimeArtifactId, RuntimeArtifactMetadataBuildError,
};

pub(crate) fn validate(
    components: &[RuntimeArtifactComponentMetadata],
) -> Result<(), RuntimeArtifactMetadataBuildError> {
    for component in components {
        for dependency in component.dependencies() {
            let valid = dependency != component.identity()
                && components.iter().any(|candidate| {
                    candidate.identity() == dependency && candidate.purpose() == component.purpose()
                });

            if !valid {
                return Err(
                    RuntimeArtifactMetadataBuildError::InvalidComponentDependency {
                        component: component.identity().clone(),
                        dependency: dependency.clone(),
                    },
                );
            }
        }

        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();

        validate_path(component, components, &mut visiting, &mut visited)?;
    }

    for component in components
        .iter()
        .filter(|component| component.roles().is_empty() && component.capabilities().is_empty())
    {
        if !components.iter().any(|candidate| {
            candidate.purpose() == component.purpose()
                && candidate.dependencies().contains(component.identity())
        }) {
            return Err(
                RuntimeArtifactMetadataBuildError::UnreferencedSupportComponent(
                    component.identity().clone(),
                ),
            );
        }
    }

    Ok(())
}

fn validate_path<'component>(
    component: &'component RuntimeArtifactComponentMetadata,
    components: &'component [RuntimeArtifactComponentMetadata],
    visiting: &mut BTreeSet<&'component RuntimeArtifactId>,
    visited: &mut BTreeSet<&'component RuntimeArtifactId>,
) -> Result<(), RuntimeArtifactMetadataBuildError> {
    if visited.contains(component.identity()) {
        return Ok(());
    }

    if !visiting.insert(component.identity()) {
        return Err(RuntimeArtifactMetadataBuildError::ComponentDependencyCycle(
            component.identity().clone(),
        ));
    }

    for dependency in component.dependencies() {
        let Some(dependency) = components.iter().find(|candidate| {
            candidate.identity() == dependency && candidate.purpose() == component.purpose()
        }) else {
            continue;
        };

        validate_path(dependency, components, visiting, visited)?;
    }

    visiting.remove(component.identity());
    visited.insert(component.identity());

    Ok(())
}
