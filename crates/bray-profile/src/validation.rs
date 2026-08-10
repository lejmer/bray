use std::collections::BTreeSet;

use crate::{COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileReport};

/// Structural reason a compiler profile report cannot be analyzed safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilationProfileValidationError {
    /// The report uses a schema revision unsupported by this toolchain.
    SchemaRevision,
    /// A descriptor catalog contains a duplicate numeric identity.
    DuplicateDescriptor,
    /// Sparse observations contain a duplicate descriptor reference.
    DuplicateObservation,
    /// An observation or event references an undeclared descriptor.
    UnknownDescriptor,
    /// A selected runtime artifact has no stable identity.
    InvalidRuntimeArtifactIdentity,
    /// Selected runtime artifacts are duplicated or not in canonical identity order.
    NonCanonicalRuntimeArtifacts,
}

impl CompilationProfileReport {
    /// Validates schema compatibility and descriptor references.
    pub fn validate(&self) -> Result<(), CompilationProfileValidationError> {
        if self.schema_revision != COMPILATION_PROFILE_SCHEMA_REVISION {
            return Err(CompilationProfileValidationError::SchemaRevision);
        }

        validate_unique(self.descriptors.operations.iter().map(|entry| entry.id))?;
        validate_unique(self.descriptors.queries.iter().map(|entry| entry.id))?;
        validate_unique(self.descriptors.metrics.iter().map(|entry| entry.id))?;

        validate_observations(
            self.operations.iter().map(|entry| entry.id),
            self.descriptors.operations.iter().map(|entry| entry.id),
        )?;

        validate_observations(
            self.queries.iter().map(|entry| entry.id),
            self.descriptors.queries.iter().map(|entry| entry.id),
        )?;

        validate_observations(
            self.metrics.iter().map(|entry| entry.id),
            self.descriptors.metrics.iter().map(|entry| entry.id),
        )?;

        if self
            .runtime_artifacts
            .iter()
            .any(|artifact| artifact.identity.trim().is_empty())
        {
            return Err(CompilationProfileValidationError::InvalidRuntimeArtifactIdentity);
        }

        if self
            .runtime_artifacts
            .windows(2)
            .any(|entries| entries[0].identity >= entries[1].identity)
        {
            return Err(CompilationProfileValidationError::NonCanonicalRuntimeArtifacts);
        }

        if self.events.iter().any(|event| {
            self.operation_descriptor(event.operation_id).is_none()
                || event
                    .query_id
                    .is_some_and(|id| self.query_descriptor(id).is_none())
        }) {
            return Err(CompilationProfileValidationError::UnknownDescriptor);
        }

        Ok(())
    }
}

fn validate_unique(
    ids: impl IntoIterator<Item = u16>,
) -> Result<(), CompilationProfileValidationError> {
    let mut observed = BTreeSet::new();

    if ids.into_iter().all(|id| observed.insert(id)) {
        Ok(())
    } else {
        Err(CompilationProfileValidationError::DuplicateDescriptor)
    }
}

fn validate_observations(
    observations: impl IntoIterator<Item = u16>,
    descriptors: impl IntoIterator<Item = u16>,
) -> Result<(), CompilationProfileValidationError> {
    let descriptors = descriptors.into_iter().collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();

    for id in observations {
        if !observed.insert(id) {
            return Err(CompilationProfileValidationError::DuplicateObservation);
        }

        if !descriptors.contains(&id) {
            return Err(CompilationProfileValidationError::UnknownDescriptor);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::CompilationProfileValidationError;
    use crate::test_support::report;

    #[test]
    fn validation_rejects_unknown_and_duplicate_observation_descriptors() {
        let mut unknown = report(1_000_000);
        unknown.operations[0].id = 99;

        assert_eq!(
            unknown.validate(),
            Err(CompilationProfileValidationError::UnknownDescriptor)
        );

        let mut duplicate = report(1_000_000);
        duplicate.operations.push(duplicate.operations[0].clone());

        assert_eq!(
            duplicate.validate(),
            Err(CompilationProfileValidationError::DuplicateObservation)
        );
    }

    #[test]
    fn validation_requires_canonical_runtime_artifact_identities() {
        let mut invalid = report(1_000_000);

        invalid.runtime_artifacts = vec![crate::CompilationProfileRuntimeArtifact {
            identity: " ".to_owned(),
            bytes: 1,
        }];

        assert_eq!(
            invalid.validate(),
            Err(CompilationProfileValidationError::InvalidRuntimeArtifactIdentity)
        );

        let mut duplicate = report(1_000_000);

        duplicate.runtime_artifacts = vec![
            crate::CompilationProfileRuntimeArtifact {
                identity: "runtime.host".to_owned(),
                bytes: 1,
            },
            crate::CompilationProfileRuntimeArtifact {
                identity: "runtime.host".to_owned(),
                bytes: 1,
            },
        ];

        assert_eq!(
            duplicate.validate(),
            Err(CompilationProfileValidationError::NonCanonicalRuntimeArtifacts)
        );
    }
}
