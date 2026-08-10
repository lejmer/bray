use std::path::Path;

use bray_base::sorted_unique_shared_slice;
use bray_target::{TargetFactKind, TargetIdentity};

use crate::manifest::{TargetPredicateManifest, TargetPredicateValueManifest};
use crate::{
    ProjectLoadError, ProjectManifestProblem, ProjectTarget, TargetPredicate, TargetPredicateValue,
};

pub(super) fn normalize_target_predicate(
    manifest: TargetPredicateManifest,
    selected_targets: &[TargetIdentity],
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<TargetPredicate, ProjectLoadError> {
    match manifest {
        TargetPredicateManifest::All(manifest) => manifest
            .all
            .into_iter()
            .map(|child| {
                normalize_target_predicate(child, selected_targets, targets, manifest_path)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(sorted_unique_shared_slice)
            .map(TargetPredicate::All),
        TargetPredicateManifest::Any(manifest) => manifest
            .any
            .into_iter()
            .map(|child| {
                normalize_target_predicate(child, selected_targets, targets, manifest_path)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(sorted_unique_shared_slice)
            .map(TargetPredicate::Any),
        TargetPredicateManifest::Not(manifest) => {
            normalize_target_predicate(*manifest.not, selected_targets, targets, manifest_path)
                .map(Box::new)
                .map(TargetPredicate::Not)
        }
        TargetPredicateManifest::Equals(manifest) => {
            let (property, value) = normalize_target_comparison(
                manifest.property,
                manifest.equals,
                selected_targets,
                targets,
                manifest_path,
            )?;

            Ok(TargetPredicate::Equals(property, value))
        }
        TargetPredicateManifest::NotEquals(manifest) => {
            let (property, value) = normalize_target_comparison(
                manifest.property,
                manifest.not_equals,
                selected_targets,
                targets,
                manifest_path,
            )?;

            Ok(TargetPredicate::NotEquals(property, value))
        }
        TargetPredicateManifest::In(manifest) => {
            let property = target_property(&manifest.property, manifest_path)?;

            let values = manifest
                .values
                .into_iter()
                .map(target_predicate_value)
                .collect::<Vec<_>>();

            validate_target_predicate_values(
                property,
                &values,
                selected_targets,
                targets,
                manifest_path,
            )?;

            Ok(TargetPredicate::In(
                property,
                sorted_unique_shared_slice(values),
            ))
        }
    }
}

fn normalize_target_comparison(
    property: String,
    value: TargetPredicateValueManifest,
    selected_targets: &[TargetIdentity],
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<(TargetFactKind, TargetPredicateValue), ProjectLoadError> {
    let property = target_property(&property, manifest_path)?;
    let value = target_predicate_value(value);

    validate_target_predicate_values(
        property,
        std::slice::from_ref(&value),
        selected_targets,
        targets,
        manifest_path,
    )?;

    Ok((property, value))
}

fn target_property(
    property: &str,
    manifest_path: &Path,
) -> Result<TargetFactKind, ProjectLoadError> {
    TargetFactKind::from_path(property).ok_or_else(|| {
        ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidTargetPredicate,
            property,
        )
    })
}

fn target_predicate_value(value: TargetPredicateValueManifest) -> TargetPredicateValue {
    match value {
        TargetPredicateValueManifest::String(value) => TargetPredicateValue::String(value.into()),
        TargetPredicateValueManifest::Usize(value) => TargetPredicateValue::Usize(value),
        TargetPredicateValueManifest::Boolean(value) => TargetPredicateValue::Boolean(value),
    }
}

fn validate_target_predicate_values(
    property: TargetFactKind,
    values: &[TargetPredicateValue],
    selected_targets: &[TargetIdentity],
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<(), ProjectLoadError> {
    for target in selected_targets {
        let Some(target) = targets
            .iter()
            .find(|candidate| candidate.identity() == target)
        else {
            return Err(ProjectLoadError::invalid(
                manifest_path.to_path_buf(),
                ProjectManifestProblem::UnknownTarget,
                target.as_str(),
            ));
        };

        let value = target.profile().fact(property);

        if values.iter().any(|expected| !expected.has_kind_of(value)) {
            return Err(ProjectLoadError::invalid(
                manifest_path.to_path_buf(),
                ProjectManifestProblem::InvalidTargetPredicate,
                property.as_str(),
            ));
        }
    }

    Ok(())
}
