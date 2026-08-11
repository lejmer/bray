use std::collections::BTreeSet;

use crate::{
    RuntimeAbiRole, RuntimeArtifactComponentMetadata, RuntimeArtifactMetadataBuildError,
    RuntimeArtifactPurpose, RuntimeContract,
};

pub(crate) fn validate(
    contract: &RuntimeContract,
    components: &[RuntimeArtifactComponentMetadata],
) -> Result<(), RuntimeArtifactMetadataBuildError> {
    let mut identities = BTreeSet::new();

    for component in components {
        if !identities.insert(component.identity()) {
            return Err(RuntimeArtifactMetadataBuildError::DuplicateComponent(
                component.identity().clone(),
            ));
        }

        if let Some(role) = component
            .roles()
            .iter()
            .find(|role| contract.role_binding(**role).is_none())
        {
            return Err(RuntimeArtifactMetadataBuildError::UnknownComponentRole(
                *role,
            ));
        }

        if let Some(capability) = component
            .capabilities()
            .iter()
            .find(|capability| contract.capabilities().binary_search(capability).is_err())
        {
            return Err(RuntimeArtifactMetadataBuildError::UnknownComponentCapability(*capability));
        }

        if component.purpose() == RuntimeArtifactPurpose::Product
            && component
                .roles()
                .binary_search(&RuntimeAbiRole::TestEntrySelection)
                .is_ok()
        {
            return Err(
                RuntimeArtifactMetadataBuildError::TestRoleInProductComponent(
                    component.identity().clone(),
                ),
            );
        }
    }

    crate::component_dependency::validate(components)?;

    for purpose in RuntimeArtifactPurpose::ALL {
        validate_role_owners(contract, components, purpose)?;
        validate_capability_owners(contract, components, purpose)?;
    }

    Ok(())
}

fn validate_role_owners(
    contract: &RuntimeContract,
    components: &[RuntimeArtifactComponentMetadata],
    purpose: RuntimeArtifactPurpose,
) -> Result<(), RuntimeArtifactMetadataBuildError> {
    for binding in contract.role_bindings() {
        let role = binding.role();

        if purpose == RuntimeArtifactPurpose::Product && role == RuntimeAbiRole::TestEntrySelection
        {
            continue;
        }

        match ownership_count(components, purpose, |component| {
            component.roles().binary_search(&role).is_ok()
        }) {
            0 => {
                return Err(RuntimeArtifactMetadataBuildError::MissingRoleOwner { purpose, role });
            }
            1 => {}
            _ => {
                return Err(RuntimeArtifactMetadataBuildError::DuplicateRoleOwner {
                    purpose,
                    role,
                });
            }
        }
    }

    Ok(())
}

fn validate_capability_owners(
    contract: &RuntimeContract,
    components: &[RuntimeArtifactComponentMetadata],
    purpose: RuntimeArtifactPurpose,
) -> Result<(), RuntimeArtifactMetadataBuildError> {
    for capability in contract.capabilities() {
        match ownership_count(components, purpose, |component| {
            component.capabilities().binary_search(capability).is_ok()
        }) {
            0 => {
                return Err(RuntimeArtifactMetadataBuildError::MissingCapabilityOwner {
                    purpose,
                    capability: *capability,
                });
            }
            1 => {}
            _ => {
                return Err(
                    RuntimeArtifactMetadataBuildError::DuplicateCapabilityOwner {
                        purpose,
                        capability: *capability,
                    },
                );
            }
        }
    }

    Ok(())
}

fn ownership_count(
    components: &[RuntimeArtifactComponentMetadata],
    purpose: RuntimeArtifactPurpose,
    owns: impl Fn(&RuntimeArtifactComponentMetadata) -> bool,
) -> usize {
    components
        .iter()
        .filter(|component| component.purpose() == purpose && owns(component))
        .count()
}
